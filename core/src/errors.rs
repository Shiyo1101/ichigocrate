// Copyright 2014-2024 the IchigoJam authors. All rights reserved. MIT license.
// https://github.com/IchigoJam/ichigojam-firm/blob/main/IchigoJam_BASIC/error.h

//! BASIC エラー型と内部伝搬用の Result 別名。
//!
//! メッセージ文言は `Display` で出力する (thiserror が生成)。
//! 数値コードは IchigoJam 標準と互換 (1..=12)。

use thiserror::Error;

/// BASIC 実行時エラー。`Break` は ESC キーによる中断。
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BasicError {
    #[error("Syntax error")]
    SyntaxError,
    #[error("Out of memory")]
    OutOfMemory,
    #[error("Stack overflow")]
    StackOverflow,
    #[error("Not match")]
    NotMatch,
    #[error("Line error")]
    UndefinedLine,
    #[error("Divide by 0")]
    DivideByZero,
    #[error("Index out of range")]
    IndexOutOfRange,
    #[error("File error")]
    FileError,
    #[error("Segmentation Fault")]
    SegmentationFault,
    #[error("Complex expression")]
    ComplexExpression,
    #[error("Illegal argument")]
    IllegalArgument,
    #[error("Break")]
    Break,
}

impl BasicError {
    /// IchigoJam 標準の番号 (1..=12)。`Machine.err` の値と対応。
    pub const fn code(self) -> u8 {
        match self {
            Self::SyntaxError => 1,
            Self::OutOfMemory => 2,
            Self::StackOverflow => 3,
            Self::NotMatch => 4,
            Self::UndefinedLine => 5,
            Self::DivideByZero => 6,
            Self::IndexOutOfRange => 7,
            Self::FileError => 8,
            Self::SegmentationFault => 9,
            Self::ComplexExpression => 10,
            Self::IllegalArgument => 11,
            Self::Break => 12,
        }
    }

    /// 数値コードからエラーへ復元。`0` なら `None` (エラー無し)。
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::SyntaxError),
            2 => Some(Self::OutOfMemory),
            3 => Some(Self::StackOverflow),
            4 => Some(Self::NotMatch),
            5 => Some(Self::UndefinedLine),
            6 => Some(Self::DivideByZero),
            7 => Some(Self::IndexOutOfRange),
            8 => Some(Self::FileError),
            9 => Some(Self::SegmentationFault),
            10 => Some(Self::ComplexExpression),
            11 => Some(Self::IllegalArgument),
            12 => Some(Self::Break),
            _ => None,
        }
    }
}

/// 内部エラー伝搬用の `Result` 別名。各 `command_*` / `token_*` は `?` で
/// 上位 (`Machine::basic_step`) まで返し、表示はそこに集約する。
pub(crate) type BResult<T> = Result<T, BasicError>;

pub(crate) const ERR_SYNTAX_ERROR: BasicError = BasicError::SyntaxError;
pub(crate) const ERR_OUT_OF_MEMORY: BasicError = BasicError::OutOfMemory;
pub(crate) const ERR_STACK_OVERFLOW: BasicError = BasicError::StackOverflow;
pub(crate) const ERR_NOT_MATCH: BasicError = BasicError::NotMatch;
pub(crate) const ERR_UNDEFINED_LINE: BasicError = BasicError::UndefinedLine;
pub(crate) const ERR_DIVIDE_BY_ZERO: BasicError = BasicError::DivideByZero;
pub(crate) const ERR_INDEX_OUT_OF_RANGE: BasicError = BasicError::IndexOutOfRange;
pub(crate) const ERR_FILE_ERROR: BasicError = BasicError::FileError;
pub(crate) const ERR_ILLEGAL_ARGUMENT: BasicError = BasicError::IllegalArgument;
pub(crate) const ERR_BREAK: BasicError = BasicError::Break;
