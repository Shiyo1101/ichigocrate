//! 制御構文のテスト: FOR/NEXT, GOTO/IF, @LABEL ジャンプ, END/STOP, CONT。

mod common;

use common::screen_text;
use ichigojam_core::{exec_line, run_to_completion, Machine, PC_NULL};

#[test]
fn for_next_loop() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 FOR I=1 TO 3");
    let _ = exec_line(&mut m, "20 ?I");
    let _ = exec_line(&mut m, "30 NEXT");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    let t = screen_text(&m);
    assert!(t.contains("1"), "{t}");
    assert!(t.contains("2"), "{t}");
    assert!(t.contains("3"), "{t}");
}

#[test]
fn goto_and_if() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 A=5");
    let _ = exec_line(&mut m, "20 IF A=5 GOTO 40");
    let _ = exec_line(&mut m, "30 ?\"NO\"");
    let _ = exec_line(&mut m, "40 ?\"YES\"");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    let t = screen_text(&m);
    assert!(t.contains("YES"), "{t}");
    assert!(!t.contains("NO"), "{t}");
}

/// `@LABEL` を文として書くと、`:` または行末までコメント扱いになる
/// (1.2b40 仕様)。
#[test]
fn at_label_statement_is_comment() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 @TOP:?\"HIT\"");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    // `:` 以降は実行されるので "HIT" が出る
    assert!(screen_text(&m).contains("HIT"));
}

/// `GOTO @LABEL` は LIST から `@LABEL` 行を探してその行へ飛ぶ。
#[test]
fn goto_at_label_jumps_to_label_line() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 GOTO @SKIP");
    let _ = exec_line(&mut m, "20 ?\"NO\"");
    let _ = exec_line(&mut m, "30 @SKIP");
    let _ = exec_line(&mut m, "40 ?\"YES\"");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    assert!(screen_text(&m).contains("YES"));
    assert!(!screen_text(&m).contains("NO"));
}

/// END / STOP: PC を NULL に戻す
#[test]
fn end_resets_pc() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 END");
    let _ = exec_line(&mut m, "20 ?\"never\"");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    assert_eq!(m.pc, PC_NULL);
    // 20 行に到達していないこと
    assert!(!screen_text(&m).contains("never"));
}

/// CONT: ESC ブレーク後、停止行から再開する。
/// STOP/END は `pcbreak` も NULL にするため CONT で再開できないのが仕様。
/// CONT が効くのは「ESC でブレークしたとき」だけ。
#[test]
fn cont_resumes_from_esc_break() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 A=A+1");
    let _ = exec_line(&mut m, "20 IF A<100 GOTO 10");
    let _ = exec_line(&mut m, "30 ?\"DONE\"");
    let _ = exec_line(&mut m, "RUN");
    // 数ステップ実行したところで ESC を入れて中断させる
    for _ in 0..5 {
        if m.basic_step().is_some() {
            break;
        }
    }
    m.is_esc_pressed = true;
    while m.pc != PC_NULL {
        if m.basic_step().is_some() {
            break;
        }
    }
    m.is_esc_pressed = false;
    // ESC ブレーク後は pcbreak に位置が記録され、CONT で再開できる。
    // ループ条件 A<100 が成立する限り 10→20 を繰り返し、最後に 30 で DONE。
    let _ = exec_line(&mut m, "CONT");
    run_to_completion(&mut m);
    assert!(
        screen_text(&m).contains("DONE"),
        "CONT should resume the loop until A>=100, then print DONE"
    );
}

/// STOP は pc・pcbreak をともに NULL に戻すため、その後の CONT は
/// 無効化される。
#[test]
fn stop_disables_subsequent_cont() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 STOP");
    let _ = exec_line(&mut m, "20 ?\"AFTER\"");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    assert_eq!(m.pc, PC_NULL);
    let _ = exec_line(&mut m, "CONT");
    run_to_completion(&mut m);
    // STOP は pcbreak=NULL にするので CONT しても 20 行へは進まない
    assert!(!screen_text(&m).contains("AFTER"));
}
