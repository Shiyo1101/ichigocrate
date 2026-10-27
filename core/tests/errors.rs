//! エラー系のテスト: エラー番号の対応、プログラム実行時/即時実行時に
//! 各エラーが正しい種別で報告されること。

mod common;

use common::screen_text;
use ichigocrate_core::{exec_line, run_to_completion, BasicError, Machine};

/// `program` を LIST に入れて RUN し、停止理由のエラーを返す。
fn run_program(program: &[&str]) -> (Machine, Option<BasicError>) {
    let mut m = Machine::new();
    for line in program {
        exec_line(&mut m, line).unwrap();
    }
    exec_line(&mut m, "RUN").unwrap();
    run_to_completion(&mut m);
    let err = m.last_error();
    (m, err)
}

/// エラー番号 (1..=12) は IchigoJam 標準と互換で、相互変換できる。
#[test]
fn error_code_round_trip() {
    for code in 1..=12 {
        let e = BasicError::from_code(code).expect("1..=12 は定義済み");
        assert_eq!(e.code(), code);
    }
    assert_eq!(BasicError::from_code(0), None);
    assert_eq!(BasicError::from_code(13), None);
}

#[test]
fn runtime_errors_report_expected_kind() {
    let cases: [(&[&str], BasicError); 9] = [
        (&["10 RETURN"], BasicError::NotMatch),
        (&["10 NEXT"], BasicError::NotMatch),
        (&["10 GOTO 99"], BasicError::UndefinedLine),
        (&["10 GOSUB 99"], BasicError::UndefinedLine),
        (&["10 GOSUB 10"], BasicError::StackOverflow),
        (&["10 FOR I=5 TO 1"], BasicError::IllegalArgument),
        (&["10 [102]=1"], BasicError::IndexOutOfRange),
        (&["10 A=1/0"], BasicError::DivideByZero),
        (&["10 A="], BasicError::SyntaxError),
    ];
    for (program, expected) in cases {
        let (_, err) = run_program(program);
        assert_eq!(err, Some(expected), "{program:?}");
    }
}

/// FOR のネストは 6 段まで。7 段目で Stack overflow。
#[test]
fn for_nesting_limit() {
    let (_, err) = run_program(&[
        "10 FOR A=0 TO 1:FOR B=0 TO 1:FOR C=0 TO 1:FOR D=0 TO 1:FOR E=0 TO 1:FOR F=0 TO 1:END",
    ]);
    assert_eq!(err, None, "6 段は許容される");
    let (_, err) = run_program(&["10 FOR A=0 TO 1:FOR B=0 TO 1:FOR C=0 TO 1:FOR D=0 TO 1:FOR E=0 TO 1:FOR F=0 TO 1:FOR G=0 TO 1"]);
    assert_eq!(err, Some(BasicError::StackOverflow));
}

/// 即時実行のエラーは `exec_line` の `Err` として返る。
#[test]
fn immediate_errors_are_returned() {
    let mut m = Machine::new();
    assert_eq!(exec_line(&mut m, "RETURN"), Err(BasicError::NotMatch));
    assert_eq!(exec_line(&mut m, "GOTO 10"), Err(BasicError::UndefinedLine));
    assert_eq!(exec_line(&mut m, "FOOBAR"), Err(BasicError::SyntaxError));
}

#[test]
fn error_message_printed_at_repl() {
    let mut m = Machine::new();
    assert_eq!(exec_line(&mut m, "?1/0"), Err(BasicError::DivideByZero));
    assert!(
        screen_text(&m).contains("Divide by 0"),
        "{}",
        screen_text(&m)
    );
}

/// プログラム実行中のエラーは「メッセージ + in <行番号>」を表示する。
#[test]
fn error_in_program_shows_line_number() {
    let (m, _) = run_program(&["10 ?1/0"]);
    let t = screen_text(&m);
    assert!(t.contains("Divide by 0 in 10"), "{t}");
}
