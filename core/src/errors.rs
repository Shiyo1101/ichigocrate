//! BASIC エラー (元 C 実装 `error.h` を Rust の列挙型に置換)。
//!
//! 内部処理は依然として `Machine.err: u8` のフラグを伝搬させているが、
//! 公開 API 境界 (`exec_line` など) では [`BasicError`] を `Result` で
//! 返す。

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

impl BasicError {
    /// 元 C 実装と同じ番号 (1..=12)。`Machine.err` の値と対応。
    pub const fn code(self) -> u8 {
        match self {
            BasicError::SyntaxError => 1,
            BasicError::OutOfMemory => 2,
            BasicError::StackOverflow => 3,
            BasicError::NotMatch => 4,
            BasicError::UndefinedLine => 5,
            BasicError::DivideByZero => 6,
            BasicError::IndexOutOfRange => 7,
            BasicError::FileError => 8,
            BasicError::SegmentationFault => 9,
            BasicError::ComplexExpression => 10,
            BasicError::IllegalArgument => 11,
            BasicError::Break => 12,
        }
    }

    /// 元 C 実装の番号からエラーへ復元。`0` なら `None` (エラー無し)。
    pub const fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            1 => BasicError::SyntaxError,
            2 => BasicError::OutOfMemory,
            3 => BasicError::StackOverflow,
            4 => BasicError::NotMatch,
            5 => BasicError::UndefinedLine,
            6 => BasicError::DivideByZero,
            7 => BasicError::IndexOutOfRange,
            8 => BasicError::FileError,
            9 => BasicError::SegmentationFault,
            10 => BasicError::ComplexExpression,
            11 => BasicError::IllegalArgument,
            12 => BasicError::Break,
            _ => return None,
        })
    }

    /// IchigoJam 標準のメッセージ文言。
    pub const fn message(self) -> &'static str {
        match self {
            BasicError::SyntaxError => "Syntax error",
            BasicError::OutOfMemory => "Out of memory",
            BasicError::StackOverflow => "Stack overflow",
            BasicError::NotMatch => "Not match",
            BasicError::UndefinedLine => "Line error",
            BasicError::DivideByZero => "Divide by 0",
            BasicError::IndexOutOfRange => "Index out of range",
            BasicError::FileError => "File error",
            BasicError::SegmentationFault => "Segmentation Fault",
            BasicError::ComplexExpression => "Complex expression",
            BasicError::IllegalArgument => "Illegal argument",
            BasicError::Break => "Break",
        }
    }
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

/// `basic_print_error` で添字参照されているメッセージ表。
/// `BasicError::from_code(i)?.message()` と同等の内容。
pub(crate) const ERR_MESSAGES: &[&str] = &[
    "",
    "Syntax error",
    "Out of memory",
    "Stack overflow",
    "Not match",
    "Line error",
    "Divide by 0",
    "Index out of range",
    "File error",
    "Segmentation Fault",
    "Complex expression",
    "Illegal argument",
    "Break",
];
