//! 式評価のテスト: 演算子の優先順位・結合、16bit 整数の丸め/ラップ、
//! 比較/論理/ビット演算、数値リテラル、組込み関数、配列添字。

mod common;

use common::var;
use ichigocrate_core::{exec_line, BasicError, Machine};

/// 式 `src` を評価した値を返す。`A=<src>` を実行して変数 A を読む。
fn eval(src: &str) -> i16 {
    let mut m = Machine::new();
    exec_line(&mut m, &format!("A={src}")).unwrap_or_else(|e| panic!("{src}: {e}"));
    var(&m, b'A')
}

#[test]
fn arithmetic_precedence_and_left_associativity() {
    assert_eq!(eval("1+2*3"), 7);
    assert_eq!(eval("(1+2)*3"), 9);
    assert_eq!(eval("10-4-3"), 3);
    assert_eq!(eval("100/10/5"), 2);
}

/// 本家 BASIC 準拠で `&` `^` `<<` `>>` は乗除算と、`|` は加減算と同順位。
#[test]
fn bitwise_operators_share_arithmetic_precedence() {
    assert_eq!(eval("2+3&1"), 3); // 2+(3&1)
    assert_eq!(eval("1+2|4"), 7); // (1+2)|4
    assert_eq!(eval("1|2*2"), 5); // 1|(2*2)
}

/// 比較は加減算より、`&&` は比較より、`||` は `&&` より弱い。
#[test]
fn comparison_and_logical_precedence() {
    assert_eq!(eval("1+1=2"), 1);
    assert_eq!(eval("1||0&&0"), 1);
    assert_eq!(eval("(1||0)&&0"), 0);
}

#[test]
fn division_and_modulo_truncate_toward_zero() {
    assert_eq!(eval("7/2"), 3);
    assert_eq!(eval("-7/2"), -3);
    assert_eq!(eval("7%3"), 1);
    assert_eq!(eval("-7%2"), -1);
    assert_eq!(eval("7 MOD 3"), 1);
}

#[test]
fn division_by_zero_is_an_error() {
    let mut m = Machine::new();
    assert_eq!(exec_line(&mut m, "A=1/0"), Err(BasicError::DivideByZero));
    assert_eq!(exec_line(&mut m, "A=1%0"), Err(BasicError::DivideByZero));
}

/// 16bit 符号付き整数としてラップする (パニックしない)。
#[test]
fn arithmetic_wraps_at_16_bits() {
    assert_eq!(eval("32767+1"), -32768);
    assert_eq!(eval("-32767-2"), 32767);
    assert_eq!(eval("256*256"), 0);
}

#[test]
fn comparison_operators_yield_one_or_zero() {
    let cases = [
        ("3=3", 1),
        ("3==3", 1),
        ("3!=3", 0),
        ("3<>4", 1),
        ("3><4", 1),
        ("3<4", 1),
        ("4<3", 0),
        ("4>3", 1),
        ("3<=3", 1),
        ("3>=4", 0),
    ];
    for (src, expected) in cases {
        assert_eq!(eval(src), expected, "{src}");
    }
}

/// `AND` `OR` `NOT` `!` は論理演算 (0/非0)、`~` はビット反転。
#[test]
fn logical_keywords_and_unary_operators() {
    assert_eq!(eval("5 AND 3"), 1);
    assert_eq!(eval("0 OR 0"), 0);
    assert_eq!(eval("NOT 5"), 0);
    assert_eq!(eval("!0"), 1);
    assert_eq!(eval("~0"), -1);
    assert_eq!(eval("-5"), -5);
    assert_eq!(eval("--3"), 3);
}

#[test]
fn bitwise_operators() {
    assert_eq!(eval("12&10"), 8);
    assert_eq!(eval("12|3"), 15);
    assert_eq!(eval("12^10"), 6);
    assert_eq!(eval("1<<4"), 16);
}

/// 右シフトは論理シフト、シフト量 16 以上は 0、負のシフト量は方向反転。
#[test]
fn shift_edge_cases() {
    assert_eq!(eval("-1>>15"), 1);
    assert_eq!(eval("1<<16"), 0);
    assert_eq!(eval("-1>>16"), 0);
    assert_eq!(eval("1>>-2"), 4);
    assert_eq!(eval("8<<-2"), 2);
}

#[test]
fn numeric_literals() {
    assert_eq!(eval("#FF"), 255);
    assert_eq!(eval("#ff"), 255);
    assert_eq!(eval("`101"), 5);
}

/// ABS は -32768 を表現できないため -32768 のまま返す (16bit ラップ)。
#[test]
fn abs_function() {
    assert_eq!(eval("ABS(-5)"), 5);
    assert_eq!(eval("ABS(5)"), 5);
    assert_eq!(eval("ABS(-32768)"), -32768);
}

/// SIN/COS は度数を受け、-256..=256 の固定小数点で返す。
#[test]
fn sin_cos_fixed_point_degrees() {
    assert_eq!(eval("SIN(0)"), 0);
    assert_eq!(eval("SIN(90)"), 256);
    assert_eq!(eval("SIN(270)"), -256);
    assert_eq!(eval("SIN(-90)"), -256);
    assert_eq!(eval("SIN(450)"), 256);
    assert_eq!(eval("COS(0)"), 256);
    assert_eq!(eval("COS(180)"), -256);
}

/// RND(n) は 0..n の範囲。n<=0 なら 0。
#[test]
fn rnd_stays_in_range() {
    let mut m = Machine::new();
    for _ in 0..500 {
        exec_line(&mut m, "A=RND(10)").unwrap();
        assert!((0..10).contains(&var(&m, b'A')));
    }
    exec_line(&mut m, "A=RND(0):B=RND(-1)").unwrap();
    assert_eq!((var(&m, b'A'), var(&m, b'B')), (0, 0));
}

/// ASC は PEEK の別名。文字列リテラルはそのアドレスに評価されるので先頭文字の
/// コードを返す。空文字列では閉じ `"` (34) を読む (元ファーム準拠)。
#[test]
fn asc_returns_first_char_code() {
    assert_eq!(eval("ASC(\"A\")"), 65);
    assert_eq!(eval("ASC(\"ABC\")"), 65);
    assert_eq!(eval("ASC(\"\")"), 34);
    assert_eq!(eval("ASC(#700)"), eval("PEEK(#700)"));
}

#[test]
fn len_counts_string_literal() {
    assert_eq!(eval("LEN(\"ABC\")"), 3);
    assert_eq!(eval("LEN(\"\")"), 0);
}

#[test]
fn pos_reports_cursor_position() {
    let mut m = Machine::new();
    exec_line(&mut m, "LOCATE 5,3").unwrap();
    exec_line(&mut m, "A=POS(1):B=POS(2)").unwrap();
    assert_eq!((var(&m, b'A'), var(&m, b'B')), (5, 3));
}

/// 配列添字は 0..=101。範囲外は Index out of range。
#[test]
fn array_index_bounds() {
    let mut m = Machine::new();
    exec_line(&mut m, "[0]=1:[101]=7").unwrap();
    assert_eq!((m.var_get(0), m.var_get(101)), (1, 7));
    assert_eq!(
        exec_line(&mut m, "[102]=1"),
        Err(BasicError::IndexOutOfRange)
    );
    assert_eq!(
        exec_line(&mut m, "[-1]=1"),
        Err(BasicError::IndexOutOfRange)
    );
}

/// `LET [i],v0,v1,...` は配列へ連続代入し、末尾を越えると Index out of range。
#[test]
fn let_array_chain_assigns_consecutive_elements() {
    let mut m = Machine::new();
    exec_line(&mut m, "LET [0],1,2,3").unwrap();
    assert_eq!((m.var_get(0), m.var_get(1), m.var_get(2)), (1, 2, 3));
    assert_eq!(
        exec_line(&mut m, "LET [100],1,2,3"),
        Err(BasicError::IndexOutOfRange)
    );
}
