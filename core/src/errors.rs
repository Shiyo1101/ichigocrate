//! BASIC エラー (元 C 実装 `error.h` を Rust の列挙型に置換)。
//!
//! 内部処理は依然として `Machine.err: u8` のフラグを伝搬させているが、
//! 公開 API 境界 (`exec_line` など) では [`BasicError`] を `Result` で
//! 返す。
//!
//! コード番号 ⇔ 列挙子 ⇔ メッセージ文言の対応は [`ERROR_TABLE`] を唯一
//! の真実とし、他のメソッドは全てこれを参照する。

use std::fmt;

/// BASIC 実行時エラー。`Break` は ESC キーによる中断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BasicError {
    SyntaxError,
    OutOfMemory,
    StackOverflow,
    NotMatch,
    UndefinedLine,
    DivideByZero,
    IndexOutOfRange,
    FileError,
    SegmentationFault,
    ComplexExpression,
    IllegalArgument,
    Break,
}

/// `(コード番号, 列挙子, IchigoJam 標準メッセージ)` の対応表。
/// インデックス 0 は「エラー無し」を示すダミー (空文字列)。
const ERROR_TABLE: &[(u8, Option<BasicError>, &str)] = &[
    (0, None, ""),
    (1, Some(BasicError::SyntaxError), "Syntax error"),
    (2, Some(BasicError::OutOfMemory), "Out of memory"),
    (3, Some(BasicError::StackOverflow), "Stack overflow"),
    (4, Some(BasicError::NotMatch), "Not match"),
    (5, Some(BasicError::UndefinedLine), "Line error"),
    (6, Some(BasicError::DivideByZero), "Divide by 0"),
    (7, Some(BasicError::IndexOutOfRange), "Index out of range"),
    (8, Some(BasicError::FileError), "File error"),
    (9, Some(BasicError::SegmentationFault), "Segmentation Fault"),
    (10, Some(BasicError::ComplexExpression), "Complex expression"),
    (11, Some(BasicError::IllegalArgument), "Illegal argument"),
    (12, Some(BasicError::Break), "Break"),
];

impl BasicError {
    /// 元 C 実装と同じ番号 (1..=12)。`Machine.err` の値と対応。
    pub const fn code(self) -> u8 {
        // const 文脈で iter を使えないため線形検索を手で展開
        let mut i = 1;
        while i < ERROR_TABLE.len() {
            if let (n, Some(e), _) = ERROR_TABLE[i] {
                if matches_variant(e, self) {
                    return n;
                }
            }
            i += 1;
        }
        0
    }

    /// 元 C 実装の番号からエラーへ復元。`0` なら `None` (エラー無し)。
    pub const fn from_code(code: u8) -> Option<Self> {
        let mut i = 1;
        while i < ERROR_TABLE.len() {
            let (n, e, _) = ERROR_TABLE[i];
            if n == code {
                return e;
            }
            i += 1;
        }
        None
    }

    /// IchigoJam 標準のメッセージ文言。
    pub const fn message(self) -> &'static str {
        message_for_code(self.code())
    }
}

/// `Machine.err` (= 数値コード) からメッセージ文言を引く。
/// 範囲外の数値は空文字列を返す (元 C 実装と同じ挙動)。
pub(crate) const fn message_for_code(code: u8) -> &'static str {
    let mut i = 0;
    while i < ERROR_TABLE.len() {
        let (n, _, msg) = ERROR_TABLE[i];
        if n == code {
            return msg;
        }
        i += 1;
    }
    ""
}

/// const 関数で `BasicError` 同士を比較するためのヘルパ
/// (列挙型に `Eq` を `derive` していても `==` は const 不可)。
const fn matches_variant(a: BasicError, b: BasicError) -> bool {
    a as u8 == b as u8
}

impl fmt::Display for BasicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for BasicError {}

// ============================================================
// 後方互換: 内部実装が参照する数値定数
// ============================================================
//
// `Machine.err: u8` 経由のフラグ伝搬を維持しているため、内部処理は
// 数値で代入する。Rust らしい書き方ではないが、basic.rs を全面書換
// せずに済ませるための妥協。

pub(crate) const ERR_SYNTAX_ERROR: u8 = BasicError::SyntaxError.code();
pub(crate) const ERR_OUT_OF_MEMORY: u8 = BasicError::OutOfMemory.code();
pub(crate) const ERR_STACK_OVERFLOW: u8 = BasicError::StackOverflow.code();
pub(crate) const ERR_NOT_MATCH: u8 = BasicError::NotMatch.code();
pub(crate) const ERR_UNDEFINED_LINE: u8 = BasicError::UndefinedLine.code();
pub(crate) const ERR_DIVIDE_BY_ZERO: u8 = BasicError::DivideByZero.code();
pub(crate) const ERR_INDEX_OUT_OF_RANGE: u8 = BasicError::IndexOutOfRange.code();
pub(crate) const ERR_FILE_ERROR: u8 = BasicError::FileError.code();
pub(crate) const ERR_ILLEGAL_ARGUMENT: u8 = BasicError::IllegalArgument.code();
pub(crate) const ERR_BREAK: u8 = BasicError::Break.code();
