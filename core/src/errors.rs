//! BASIC エラーを Rust 列挙型として表現する。
//!
//! 内部処理は依然として `Machine.err: u8` のフラグを伝搬させているが、
//! 公開 API 境界 (`exec_line` など) では [`BasicError`] を `Result` で返す。
//!
//! コード番号 ⇔ 列挙子 ⇔ メッセージ文言の対応は [`ERROR_TABLE`] を唯一の
//! 真実とし、他のメソッドは全てこれを参照する。

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
    /// IchigoJam 標準の番号 (1..=12)。`Machine.err` の値と対応。
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

    /// 数値コードからエラーへ復元。`0` なら `None` (エラー無し)。
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
/// 範囲外の数値は空文字列を返す (実機準拠)。
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
// 内部実装が使うエラー伝搬の道具立て
// ============================================================

/// インタプリタ内部のエラー伝搬に使う `Result` 別名。各 `command_*` /
/// `token_*` はこれを返し、`?` 演算子でそのまま呼出元へ伝搬する。
/// 表示は最上位 ([`crate::machine::Machine::basic_step`]) に集約する。
pub(crate) type BResult<T> = Result<T, BasicError>;

// エラー値は数値コードではなく型付きの [`BasicError`] 定数として持つ。
// 既存の呼出箇所が参照する短縮名を維持するための別名。
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
