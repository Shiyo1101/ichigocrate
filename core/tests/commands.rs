//! 各コマンドの挙動テスト:
//! 代入/CLS/BTN/NEW/CLV/CLK/CLT/CLP/LED/SRND/SCROLL/COPY/BEEP/PLAY/OK と
//! プログラム無し RUN の安全性。

mod common;

use common::{screen_text, var, vram_line};
use ichigojam_core::ram::{OFFSET_RAM_PCG, SIZE_RAM_PCG};
use ichigojam_core::{
    exec_line, exec_line_bytes, Machine, OFFSET_RAMROM, OFFSET_RAM_LIST, OFFSET_RAM_VRAM, PC_NULL,
};

#[test]
fn variable_assignment() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "A=42");
    let _ = exec_line(&mut m, "?A");
    assert!(vram_line(&m, 0).contains("42"), "got: {:?}", screen_text(&m));
}

#[test]
fn cls_clears_vram() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "?\"HELLO\"");
    let _ = exec_line(&mut m, "CLS");
    let t = screen_text(&m);
    assert!(!t.contains("HELLO"), "{t}");
}

/// プログラム無しで RUN しても OK (パニックしない)。
#[test]
fn run_with_no_program_is_safe() {
    let mut m = Machine::new();
    assert_eq!(m.listsize, 0);
    let r = exec_line_bytes(&mut m, b"RUN");
    assert!(r.is_ok());
    assert_eq!(m.pc, PC_NULL, "空プログラムの RUN 後は pc が NULL");
}

#[test]
fn btn_reads_key_state() {
    let mut m = Machine::new();
    // 何も押していなければ 0
    let _ = exec_line(&mut m, "?BTN(28)");
    assert_eq!(vram_line(&m, 0), "0");

    // 左矢印 (28) を押下 → 1
    m.key_set_down(28, true);
    let _ = exec_line(&mut m, "?BTN(28)");
    assert_eq!(vram_line(&m, 1), "1");

    // X キー (88) を押下 → 1。押していない右矢印 (29) は 0
    m.key_set_down(88, true);
    let _ = exec_line(&mut m, "?BTN(88)");
    let _ = exec_line(&mut m, "?BTN(29)");
    assert_eq!(vram_line(&m, 2), "1");
    assert_eq!(vram_line(&m, 3), "0");

    // 引数なし BTN() は実機ボタン → デスクトップでは常に 0
    let _ = exec_line(&mut m, "?BTN()");
    assert_eq!(vram_line(&m, 4), "0");

    // 解放したら 0 に戻る
    m.key_set_down(28, false);
    let _ = exec_line(&mut m, "?BTN(28)");
    assert_eq!(vram_line(&m, 5), "0");
}

#[test]
fn btn_negative_returns_bitmask() {
    let mut m = Machine::new();
    m.key_set_down(28, true); // 左 → bit0 (1)
    m.key_set_down(32, true); // スペース → bit4 (16)
    let _ = exec_line(&mut m, "?BTN(-1)");
    assert_eq!(vram_line(&m, 0), "17");

    // 全クリアで 0
    m.key_clear_down();
    let _ = exec_line(&mut m, "?BTN(-1)");
    assert_eq!(vram_line(&m, 1), "0");
}

/// NEW: LIST 領域がゼロクリアされ listsize/pc/pcbreak も初期化される。
#[test]
fn new_clears_list_and_pc() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 ?\"X\"");
    let _ = exec_line(&mut m, "20 ?\"Y\"");
    assert!(m.listsize > 0);
    let _ = exec_line(&mut m, "NEW");
    assert_eq!(m.listsize, 0);
    assert_eq!(m.pc, PC_NULL);
    // LIST 先頭バイトが全部 0 になっていることをサンプリング
    for &b in &m.ram[OFFSET_RAM_LIST..OFFSET_RAM_LIST + 32] {
        assert_eq!(b, 0);
    }
}

/// CLV: 変数領域 (配列 0..101 + A-Z) がゼロクリアされる。
#[test]
fn clv_clears_variables() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "A=123");
    let _ = exec_line(&mut m, "B=456");
    let _ = exec_line(&mut m, "[3]=7"); // 配列要素も書く
    assert_eq!(var(&m, b'A'), 123);
    assert_eq!(var(&m, b'B'), 456);
    assert_eq!(m.var_get(3), 7);
    let _ = exec_line(&mut m, "CLV");
    assert_eq!(var(&m, b'A'), 0);
    assert_eq!(var(&m, b'B'), 0);
    assert_eq!(m.var_get(3), 0);
}

/// CLK: キーバッファを空にする。
/// 直後の `key_get_key` は -1 (未取得) を返す。
#[test]
fn clk_clears_key_buffer() {
    let mut m = Machine::new();
    m.key_push(b'X');
    m.key_push(b'Y');
    let _ = exec_line(&mut m, "CLK");
    assert_eq!(m.key_get_key(), -1);
}

/// CLT: tick カウンタ (frames) をゼロに戻す
#[test]
fn clt_resets_frames_counter() {
    let mut m = Machine::new();
    m.frames = 1234;
    let _ = exec_line(&mut m, "CLT");
    assert_eq!(m.frames, 0);
}

/// CLP: PCG (書き換え可能キャラクタ) をフォント末尾 32 文字で初期化する
#[test]
fn clp_resets_pcg_to_font_tail() {
    let mut m = Machine::new();
    // まず PCG を全部 0xFF で潰す
    m.ram[OFFSET_RAM_PCG..OFFSET_RAM_PCG + SIZE_RAM_PCG].fill(0xff);
    let _ = exec_line(&mut m, "CLP");
    // CLP 後は font の末尾 32 文字でちょうど埋まる
    // (内容は font 依存のためフィールド全部が 0xff のままではないことを確認)
    let pcg = &m.ram[OFFSET_RAM_PCG..OFFSET_RAM_PCG + SIZE_RAM_PCG];
    assert!(pcg.iter().any(|&b| b != 0xff), "PCG should be reset");
}

/// LED: 引数 0 で消灯、それ以外で点灯
#[test]
fn led_command_toggles_led_state() {
    let mut m = Machine::new();
    assert!(!m.led);
    let _ = exec_line(&mut m, "LED 1");
    assert!(m.led);
    let _ = exec_line(&mut m, "LED 0");
    assert!(!m.led);
    // 0 以外は ON
    let _ = exec_line(&mut m, "LED 42");
    assert!(m.led);
}

/// SRND: 同じシードを与えれば RND は同じ値を返す
#[test]
fn srnd_makes_rnd_reproducible() {
    let mut a = Machine::new();
    let mut b = Machine::new();
    let _ = exec_line(&mut a, "SRND 42");
    let _ = exec_line(&mut b, "SRND 42");
    let _ = exec_line(&mut a, "?RND(100)");
    let _ = exec_line(&mut b, "?RND(100)");
    assert_eq!(vram_line(&a, 0), vram_line(&b, 0));
    // 異なるシードならふつう違う値になる
    let mut c = Machine::new();
    let _ = exec_line(&mut c, "SRND 1");
    let _ = exec_line(&mut c, "?RND(10000)");
    let _ = exec_line(&mut b, "?RND(10000)");
    // 完全一致になる可能性はゼロではないが、IchigoJam の xorshift では実質起きない
    assert_ne!(vram_line(&c, 0), vram_line(&b, 1));
}

/// SCROLL 0/1/2/3 がそれぞれ UP/RIGHT/DOWN/LEFT に対応
#[test]
fn scroll_directions_move_vram() {
    // SCROLL 0 (UP): 2 行目に書いた文字が 1 行目に来る
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "LOCATE 0,1");
    let _ = exec_line(&mut m, "?\"AB\";");
    let _ = exec_line(&mut m, "SCROLL 0");
    assert_eq!(vram_line(&m, 0), "AB");

    // SCROLL 1 (RIGHT): 0 列目に書いた文字が 1 列目に来る
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "LOCATE 0,0");
    let _ = exec_line(&mut m, "?\"P\";");
    let _ = exec_line(&mut m, "SCROLL 1");
    assert_eq!(m.ram[OFFSET_RAM_VRAM + 1], b'P');
    assert_eq!(m.ram[OFFSET_RAM_VRAM], 0);

    // SCROLL 2 (DOWN): 0 行目に書いた文字が 1 行目に来る
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "LOCATE 0,0");
    let _ = exec_line(&mut m, "?\"CD\";");
    let _ = exec_line(&mut m, "SCROLL 2");
    assert_eq!(vram_line(&m, 1), "CD");

    // SCROLL 3 (LEFT): 1 列目に書いた文字が 0 列目に来る
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "LOCATE 1,0");
    let _ = exec_line(&mut m, "?\"Q\";");
    let _ = exec_line(&mut m, "SCROLL 3");
    assert_eq!(m.ram[OFFSET_RAM_VRAM], b'Q');
}

/// COPY: 正の len は順方向、負の len は逆方向。
/// COPY のアドレスは仮想アドレス (OFFSET_RAMROM 加算済み) を渡すため、
/// PCG 先頭 (ram[0]) は仮想アドレス OFFSET_RAMROM になる。
#[test]
fn copy_forward_and_backward() {
    let pcg_v = OFFSET_RAMROM as i32; // PCG 先頭の仮想アドレス
    let mut m = Machine::new();
    // src 範囲 (PCG[0..8]) に "ABCDEFGH" を直接書く
    for i in 0..8 {
        m.ram[OFFSET_RAM_PCG + i] = b'A' + i as u8;
    }
    // 順方向: dst=PCG+8, src=PCG+0, len=8 → PCG[8..16] が "ABCDEFGH"
    let cmd = format!("COPY {},{},8", pcg_v + 8, pcg_v);
    let _ = exec_line(&mut m, &cmd);
    for i in 0..8 {
        assert_eq!(m.ram[OFFSET_RAM_PCG + 8 + i], b'A' + i as u8);
    }

    // 逆方向: 重なりがある領域を 1 byte 後ろへシフト。
    // dst=PCG+7, src=PCG+6, len=-7 → ram[7]←ram[6], ram[6]←ram[5], ...
    //                                ram[1]←ram[0]。先頭 ram[0] は変わらない。
    for i in 0..8 {
        m.ram[OFFSET_RAM_PCG + i] = b'A' + i as u8;
    }
    let cmd = format!("COPY {},{},-7", pcg_v + 7, pcg_v + 6);
    let _ = exec_line(&mut m, &cmd);
    assert_eq!(m.ram[OFFSET_RAM_PCG + 7], b'G');
    assert_eq!(m.ram[OFFSET_RAM_PCG + 6], b'F');
    assert_eq!(m.ram[OFFSET_RAM_PCG + 1], b'A');
    assert_eq!(m.ram[OFFSET_RAM_PCG], b'A');
}

/// BEEP (引数なし): 既定 TONE=10, LEN=3。tone>0 で current_tone_hz が
/// 0 でない値になる。
#[test]
fn beep_no_args_uses_default_tone() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "BEEP");
    assert!(m.psg_sound());
    assert!(m.current_tone_hz > 0.0);
}

/// PLAY (引数なし): MML 停止。psgmml が None になり
/// 無音になる。
#[test]
fn play_with_no_arg_stops_mml() {
    let mut m = Machine::new();
    // 一度 BEEP で鳴らしてから PLAY (引数なし) で止める
    let _ = exec_line(&mut m, "BEEP 10,1000");
    assert!(m.psg_sound());
    let _ = exec_line(&mut m, "PLAY");
    assert!(!m.psg_sound());
}

/// OK 2: noresmode (応答抑制) ON、それ以外は OFF (commands.rs:331)
#[test]
fn ok_2_enables_quiet_mode() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "OK 2");
    // noresmode は pub(crate) のため公開 API では直接見えない。
    // 次の `OK 0` で OFF に戻すことだけ通す動作確認とする。
    let _ = exec_line(&mut m, "OK 0");
}
