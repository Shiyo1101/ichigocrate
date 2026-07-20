//! セッション層 (ホスト共通の実行状態機械) の結合テスト。
//!
//! かつてフロントエンド側に埋まっていて headless で検証できなかった REPL の
//! 遷移 ("OK" 表示・F キー・RUN 継続・INPUT 対話・ESC 中断) を core 単体で
//! 固定する。

mod common;

use common::screen_text;
use ichigocrate_core::session::{fkey_binding, Session, FRAME_MS};
use ichigocrate_core::{BasicError, Machine};

fn new_session() -> Session {
    Session::new(Machine::new())
}

/// 文字列を 1 文字ずつタイプする (Enter は含まない)。
fn type_str(s: &mut Session, text: &str) {
    for b in text.bytes() {
        let _ = s.feed_char(b);
    }
}

#[test]
fn starts_idle_with_ok_prompt() {
    let s = new_session();
    assert!(!s.is_running());
    assert!(!s.is_awaiting_input());
    assert!(screen_text(&s.machine).contains("OK"));
}

#[test]
fn immediate_command_prints_ok() {
    let mut s = new_session();
    type_str(&mut s, "LET A,1");
    assert_eq!(s.on_enter(), None);
    // 即時実行は Enter 内で完了し、REPL へ戻って "OK" を表示する。
    assert!(!s.is_running());
    // 起動時 + 実行後で "OK" は 2 回
    assert_eq!(screen_text(&s.machine).matches("OK").count(), 2);
}

#[test]
fn f1_cls_clears_screen_without_ok() {
    let mut s = new_session();
    // 編集途中の行が残っていても F キーは行を消してから投入する。
    type_str(&mut s, "ABC");
    assert_eq!(s.press_fkey(1), None);
    assert!(!s.is_running());
    // 画面は完全な空白のまま ("OK" が出ると空白画面にならない)。
    let text = screen_text(&s.machine);
    assert_eq!(text.trim(), "", "screen should be blank, got: {text:?}");
}

#[test]
fn typed_cls_still_prints_ok() {
    let mut s = new_session();
    // F1 ではなく手入力の CLS は通常の即時コマンドとして "OK" を表示する。
    type_str(&mut s, "CLS");
    assert_eq!(s.on_enter(), None);
    assert_eq!(screen_text(&s.machine).matches("OK").count(), 1);
}

#[test]
fn fkey_bindings_cover_f1_to_f9() {
    assert_eq!(fkey_binding(1), Some(("CLS", true)));
    assert_eq!(fkey_binding(5), Some(("RUN", true)));
    assert_eq!(fkey_binding(9), Some(("FILES", true)));
    assert_eq!(fkey_binding(0), None);
    assert_eq!(fkey_binding(10), None);
}

#[test]
fn fkey_ignored_while_running() {
    let mut s = new_session();
    let _ = s.exec_line(b"10 GOTO 10");
    let _ = s.exec_line(b"RUN");
    assert!(s.is_running());
    let before = screen_text(&s.machine);
    assert_eq!(s.press_fkey(1), None);
    assert_eq!(screen_text(&s.machine), before);
}

#[test]
fn run_infinite_loop_keeps_running_across_ticks() {
    let mut s = new_session();
    let _ = s.exec_line(b"10 GOTO 10");
    let _ = s.exec_line(b"RUN");
    assert!(s.is_running());
    // フレームを進めても無限ループは完了しない (UI を固めず継続する)。
    for i in 0..3 {
        assert_eq!(s.tick(f64::from(i) * FRAME_MS), None);
        assert!(s.is_running());
    }
}

#[test]
fn escape_breaks_running_program() {
    let mut s = new_session();
    let _ = s.exec_line(b"10 GOTO 10");
    let _ = s.exec_line(b"RUN");
    assert!(s.is_running());
    s.on_escape();
    // 停止理由として Break が返る。ユーザ操作として通知を抑えるかは
    // ホスト側の判断 (web は onError へ流さない)。
    assert_eq!(s.tick(0.0), Some(BasicError::Break));
    assert!(!s.is_running());
    assert!(screen_text(&s.machine).contains("Break in 10"));
}

#[test]
fn wait_defers_completion_to_real_time() {
    let mut s = new_session();
    type_str(&mut s, "WAIT 60");
    assert_eq!(s.on_enter(), None);
    assert!(s.is_running());
    // WAIT 発火後の tick で実時間期限 (60 フレーム = 1 秒) へ変換される。
    assert_eq!(s.tick(0.0), None);
    assert!(s.is_waiting());
    assert!(s.is_running());
    // 期限前は完了しない。
    assert_eq!(s.tick(500.0), None);
    assert!(s.is_running());
    // 期限を過ぎたフレームで完了し "OK" が出る。
    assert_eq!(s.tick(1100.0), None);
    assert!(!s.is_running());
    assert_eq!(screen_text(&s.machine).matches("OK").count(), 2);
}

#[test]
fn input_flow_assigns_typed_value() {
    let mut s = new_session();
    type_str(&mut s, "INPUT A:PRINT A*2");
    assert_eq!(s.on_enter(), None);
    assert!(s.is_awaiting_input());
    assert!(!s.is_running());
    type_str(&mut s, "21");
    assert_eq!(s.on_enter(), None);
    assert!(!s.is_awaiting_input());
    // INPUT 確定後は実行が再開され、続きの文が走る。
    assert_eq!(s.tick(0.0), None);
    assert!(!s.is_running());
    assert!(screen_text(&s.machine).contains("42"));
}

#[test]
fn escape_cancels_input_without_assign() {
    let mut s = new_session();
    type_str(&mut s, "A=7:INPUT A");
    assert_eq!(s.on_enter(), None);
    assert!(s.is_awaiting_input());
    s.on_escape();
    assert!(!s.is_awaiting_input());
    assert!(!s.is_running());
    // 中断時は代入されず REPL へ戻る。
    type_str(&mut s, "PRINT A");
    assert_eq!(s.on_enter(), None);
    assert!(screen_text(&s.machine).contains('7'));
}

#[test]
fn immediate_error_is_returned_and_printed() {
    let mut s = new_session();
    type_str(&mut s, "FOOBAR");
    let err = s.on_enter();
    assert!(err.is_some(), "syntax error should be surfaced to the host");
    assert!(screen_text(&s.machine).contains("Syntax error"));
    assert!(!s.is_running());
}

#[test]
fn runtime_error_is_returned_from_tick() {
    let mut s = new_session();
    let _ = s.exec_line(b"10 A=A+1:IF A<2000 GOTO 10");
    let _ = s.exec_line(b"20 GOTO 999");
    let _ = s.exec_line(b"RUN");
    // 1 フレームでは終わらない行数を回してから未定義行番号で停止する。
    let mut err = None;
    for i in 0..10 {
        err = s.tick(f64::from(i) * FRAME_MS);
        if err.is_some() {
            break;
        }
    }
    assert!(err.is_some(), "runtime error should surface via tick");
    assert!(!s.is_running());
}

#[test]
fn line_edit_prints_no_ok() {
    let mut s = new_session();
    type_str(&mut s, "10 PRINT 1");
    assert_eq!(s.on_enter(), None);
    // 行編集 (LIST への追加) は "OK" を出さない (IchigoJam 慣習)。
    assert_eq!(screen_text(&s.machine).matches("OK").count(), 1);
}

#[test]
fn reset_returns_to_power_on_state() {
    let mut s = new_session();
    let _ = s.exec_line(b"10 GOTO 10");
    let _ = s.exec_line(b"RUN");
    assert!(s.is_running());
    s.reset();
    assert!(!s.is_running());
    assert!(!s.is_awaiting_input());
    assert!(screen_text(&s.machine).contains("OK"));
}
