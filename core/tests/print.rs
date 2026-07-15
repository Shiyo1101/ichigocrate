//! PRINT 出力と数値/文字列フォーマット関数 (HEX$ / BIN$ / DEC$) のテスト。

mod common;

use common::{screen_text, vram_line};
use ichigocrate_core::{exec_line, exec_line_bytes, Machine};

#[test]
fn print_simple_expression() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "?1+2");
    assert_eq!(vram_line(&m, 0), "3");
}

#[test]
fn print_with_string() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "?\"HI\"");
    assert_eq!(vram_line(&m, 0), "HI");
}

/// PRINT のセミコロン (`;`) は末尾改行を抑制する。
#[test]
fn print_semicolon_suppresses_newline() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?\"A\";");
    let _ = exec_line_bytes(&mut m, b"?\"B\"");
    // 改行抑制なので "A" と "B" が連結して y=0 に "AB"
    assert_eq!(vram_line(&m, 0), "AB");
}

/// PRINT のカンマ (`,`) は値の間にスペース 1 個を入れる。
#[test]
fn print_comma_inserts_space() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?1,2,3");
    assert_eq!(vram_line(&m, 0), "1 2 3");
}

/// 空入力行は何もしない (パニックしないこと)。
#[test]
fn empty_line_is_noop() {
    let mut m = Machine::new();
    let r = exec_line_bytes(&mut m, b"");
    assert!(r.is_ok());
}

#[test]
fn hex_and_bin() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "?#ff");
    let _ = exec_line(&mut m, "?`101");
    let t = screen_text(&m);
    assert!(t.contains("255"), "{t}");
    assert!(t.contains("5"), "{t}");
}

/// HEX$ で 0xFF を 2 桁指定で出力すると "FF" になる。
#[test]
fn hex_dollar_two_digits() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?HEX$(255,2)");
    assert_eq!(vram_line(&m, 0), "FF");
}

/// BIN$ で 0b1010 を 4 桁指定で出力すると "1010" になる。
#[test]
fn bin_dollar_four_digits() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?BIN$(10,4)");
    assert_eq!(vram_line(&m, 0), "1010");
}

/// DEC$ で負数の桁指定 (右寄せ) が効くこと。
#[test]
fn dec_dollar_right_justifies() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?DEC$(7,3)");
    // 3 桁右寄せ: "  7"
    assert_eq!(vram_line(&m, 0), "  7");
}
