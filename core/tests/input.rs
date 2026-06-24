//! INPUT 文の対話入力テスト。
//!
//! INPUT は即値代入をせず、プロンプトを出して入力待ちに入る。確定値の代入は
//! `input_complete` が、中断は `cancel_input` が担う。

mod common;

use common::{screen_text, var};
use ichigojam_core::{exec_line, run_to_completion, BasicResult, Machine};

/// INPUT は即値の代入はせず、プロンプトを出して入力待ちに入る。
/// 確定するまで変数は元の値のまま (代入は input_complete が担う)。
#[test]
fn input_enters_await_state_without_assigning() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "A=99");
    let _ = exec_line(&mut m, "INPUT A");
    assert!(m.is_awaiting_input());
    assert_eq!(var(&m, b'A'), 99);
}

/// プログラム実行中の INPUT は basic_step が `BasicResult::Input` を返して
/// 入力待ちに入り、`input_complete` で受け取った値を変数へ代入して実行を
/// 再開する。
#[test]
fn input_assigns_typed_value_during_run() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 INPUT A");
    let _ = exec_line(&mut m, "20 ?A*2");
    let _ = exec_line(&mut m, "RUN");

    // INPUT 文に達すると Input が返り、入力待ちになる。
    let mut hit = false;
    for _ in 0..50 {
        if let Some(r) = m.basic_step() {
            assert_eq!(r, BasicResult::Input);
            hit = true;
            break;
        }
    }
    assert!(hit, "INPUT が入力待ち (BasicResult::Input) を返さなかった");
    assert!(m.is_awaiting_input());
    // デフォルトプロンプト '?' が表示されている。
    assert!(screen_text(&m).contains('?'));

    m.input_complete(b"21");
    assert!(!m.is_awaiting_input());
    assert_eq!(var(&m, b'A'), 21);

    // 入力後は INPUT 文の直後 (?A*2) から継続して 42 を出力する。
    run_to_completion(&mut m);
    assert!(
        screen_text(&m).contains("42"),
        "INPUT 後に実行が継続していない: {:?}",
        screen_text(&m)
    );
}

/// INPUT は数値リテラルだけでなく式も入力値として評価する。
/// 文字列プロンプト付き構文 `INPUT "...",var` も確認する。
#[test]
fn input_evaluates_expression_with_prompt() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "B=3");
    // 即時モードの INPUT は AwaitingInput を返す。
    let _ = exec_line(&mut m, "INPUT \"VAL\",A");
    assert!(m.is_awaiting_input());
    assert!(screen_text(&m).contains("VAL"));

    m.input_complete(b"B+4");
    assert_eq!(var(&m, b'A'), 7);
}

/// 空入力 (Enter のみ) はパースに失敗するため、代入をスキップし、
/// 変数は元の値のまま残す。
#[test]
fn input_empty_keeps_previous_value() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "A=5");
    let _ = exec_line(&mut m, "INPUT A");
    assert!(m.is_awaiting_input());

    m.input_complete(b"");
    assert!(!m.is_awaiting_input());
    assert_eq!(var(&m, b'A'), 5);
}

/// cancel_input は代入せず入力待ちを解除する (ESC 中断相当)。
#[test]
fn cancel_input_clears_pending_without_assigning() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "A=9");
    let _ = exec_line(&mut m, "INPUT A");
    assert!(m.is_awaiting_input());

    m.cancel_input();
    assert!(!m.is_awaiting_input());
    assert_eq!(var(&m, b'A'), 9);
}
