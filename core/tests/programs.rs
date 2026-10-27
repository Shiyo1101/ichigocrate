//! より複雑なプログラム例: サブルーチン・計算・キー入力・WAIT・SAVE/LOAD・
//! DRAW/POINT・PEEK/POKE。

mod common;

use common::{var, vram_line, MemStore};
use ichigocrate_core::{exec_line, run_to_completion, BasicError, Machine, PC_NULL};

#[test]
fn gosub_return() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 ?\"START\"");
    let _ = exec_line(&mut m, "20 GOSUB 100");
    let _ = exec_line(&mut m, "30 ?\"END\"");
    let _ = exec_line(&mut m, "40 END");
    let _ = exec_line(&mut m, "100 ?\"SUB\"");
    let _ = exec_line(&mut m, "110 RETURN");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    // RETURN で呼出し直後の行へ戻るので START → SUB → END の順に並ぶ。
    assert_eq!(vram_line(&m, 0), "START");
    assert_eq!(vram_line(&m, 1), "SUB");
    assert_eq!(vram_line(&m, 2), "END");
}

#[test]
fn fib_program() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 A=0:B=1");
    let _ = exec_line(&mut m, "20 FOR I=1 TO 5");
    let _ = exec_line(&mut m, "30 ?B;\" \";");
    let _ = exec_line(&mut m, "40 C=A+B:A=B:B=C");
    let _ = exec_line(&mut m, "50 NEXT");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    assert_eq!(vram_line(&m, 0), "1 1 2 3 5 ");
}

#[test]
fn inkey_pops_buffered_keys_in_order() {
    // INKEY() はキューに積まれたキーを押下順に 1 つずつ返し、空なら 0。
    // (英字キーが取りこぼされる回帰バグの検出用)
    let mut m = Machine::new();
    m.key_push(b'A');
    m.key_push(b'B');
    let _ = exec_line(&mut m, "?INKEY()");
    let _ = exec_line(&mut m, "?INKEY()");
    let _ = exec_line(&mut m, "?INKEY()");
    assert_eq!(vram_line(&m, 0), "65", "1st INKEY() should pop 'A'");
    assert_eq!(vram_line(&m, 1), "66", "2nd INKEY() should pop 'B'");
    assert_eq!(vram_line(&m, 2), "0", "empty queue should yield 0");
}

#[test]
fn inkey_in_running_program() {
    // 実行中に押された英字キーを INKEY() が拾えること。
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 K=INKEY()");
    let _ = exec_line(&mut m, "20 IF K=0 GOTO 10");
    let _ = exec_line(&mut m, "30 END");
    let _ = exec_line(&mut m, "RUN"); // inkey_queue をクリアし LIST へ移行
                                      // キー未入力の間は 10-20 をループ。数ステップ空回しする。
    for _ in 0..50 {
        m.wait_frames = 0;
        let _ = m.basic_step();
    }
    assert_ne!(m.pc, PC_NULL, "キー入力までループし続ける");
    // プログラム実行中にキーが押された状況を模す。
    m.key_push(b'A');
    run_to_completion(&mut m);
    assert_eq!(var(&m, b'K'), 65);
}

#[test]
fn wait_and_goto_loop_yields() {
    // 元プログラム: 10 ?"ICHIGOCRATE" / 20 WAIT60 / 30 GOTO10
    // RUN 後、basic_execute は LIST へ移行した時点で呼出元へ制御を返すこと、
    // WAIT で wait_frames がセットされて以降の自動進行が止まることを確認。
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 ?\"ICHIGOCRATE\"");
    let _ = exec_line(&mut m, "20 WAIT60");
    let _ = exec_line(&mut m, "30 GOTO10");
    let _ = exec_line(&mut m, "RUN");
    // 1 周目: PRINT 実行 → WAIT で stop → wait_frames が 60
    while m.wait_frames == 0 && m.pc != PC_NULL {
        let _ = m.basic_step();
    }
    assert_eq!(m.wait_frames, 60);
    assert_eq!(vram_line(&m, 0), "ICHIGOCRATE");
}

#[test]
fn save_load_roundtrip() {
    let mut m = Machine::new();
    m.set_storage(Box::<MemStore>::default());
    let _ = exec_line(&mut m, "10 ?\"HELLO\"");
    let _ = exec_line(&mut m, "20 ?\"WORLD\"");
    let original_size = m.list_size;
    let _ = exec_line(&mut m, "SAVE 0");
    let _ = exec_line(&mut m, "NEW");
    assert_eq!(m.list_size, 0);
    let _ = exec_line(&mut m, "LOAD 0");
    assert_eq!(m.list_size, original_size);
    let _ = exec_line(&mut m, "CLS");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    assert_eq!(vram_line(&m, 0), "HELLO");
    assert_eq!(vram_line(&m, 1), "WORLD");
}

/// スロットは独立しており、未保存スロットの LOAD は File error。
/// LOAD は読込前に LIST を消去するため、失敗時も既存プログラムは残らない。
#[test]
fn load_unsaved_slot_is_file_error() {
    let mut m = Machine::new();
    m.set_storage(Box::<MemStore>::default());
    let _ = exec_line(&mut m, "10 ?\"X\"");
    let _ = exec_line(&mut m, "SAVE 1");
    assert_eq!(exec_line(&mut m, "LOAD 2"), Err(BasicError::FileError));
    assert_eq!(m.list_size, 0);
    assert!(exec_line(&mut m, "LOAD 1").is_ok());
    assert!(m.list_size > 0);
}

/// ストレージ未設定のホストでは SAVE/LOAD とも File error。
#[test]
fn save_load_without_storage_is_file_error() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 ?\"X\"");
    assert_eq!(exec_line(&mut m, "SAVE 0"), Err(BasicError::FileError));
    assert_eq!(exec_line(&mut m, "LOAD 0"), Err(BasicError::FileError));
}

#[test]
fn draw_two_args_sets_point() {
    // DRAW x,y は点を描く (cmd 既定 = 1)
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "DRAW 20,20");
    let _ = exec_line(&mut m, "?POINT(20,20)");
    assert_eq!(vram_line(&m, 0), "1");
}

#[test]
fn draw_three_args_uses_explicit_cmd() {
    // DRAW x,y,c は cmd c で点を描く (1=セット, 0=クリア)
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "DRAW 20,20,1");
    let _ = exec_line(&mut m, "?POINT(20,20)");
    let _ = exec_line(&mut m, "DRAW 20,20,0");
    let _ = exec_line(&mut m, "?POINT(20,20)");
    assert_eq!(vram_line(&m, 0), "1");
    assert_eq!(vram_line(&m, 1), "0");
}

#[test]
fn draw_four_args_draws_line() {
    // DRAW x1,y1,x2,y2 は線を描く (cmd 既定 = 1)。端点だけでなく中間点も点灯する。
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "DRAW 20,20,30,20");
    let _ = exec_line(&mut m, "?POINT(20,20)");
    let _ = exec_line(&mut m, "?POINT(25,20)");
    let _ = exec_line(&mut m, "?POINT(30,20)");
    assert_eq!(vram_line(&m, 0), "1");
    assert_eq!(vram_line(&m, 1), "1");
    assert_eq!(vram_line(&m, 2), "1");
}

#[test]
fn draw_five_args_draws_line_with_cmd() {
    // DRAW x1,y1,x2,y2,c は cmd c で線を描く。
    // 旧実装はここで pos[5] へ書き込みパニックしていた (回帰テスト)。
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "DRAW 20,20,30,30,1");
    let _ = exec_line(&mut m, "?POINT(20,20)");
    let _ = exec_line(&mut m, "?POINT(30,30)");
    assert_eq!(vram_line(&m, 0), "1");
    assert_eq!(vram_line(&m, 1), "1");
}

#[test]
fn poke_peek_pcg() {
    let mut m = Machine::new();
    // PCG 領域 (0x700) に複数バイトを連続で書き込んで読み戻し
    let _ = exec_line(&mut m, "POKE #700,#FF,#AA,#55");
    let _ = exec_line(&mut m, "?PEEK(#700)");
    let _ = exec_line(&mut m, "?PEEK(#701)");
    let _ = exec_line(&mut m, "?PEEK(#702)");
    assert_eq!(vram_line(&m, 0), "255");
    assert_eq!(vram_line(&m, 1), "170");
    assert_eq!(vram_line(&m, 2), "85");
}
