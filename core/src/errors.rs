//! BASIC エラーメッセージ (error.h より移植)

#![allow(dead_code)]

pub const ERR_NO_ERROR: u8 = 0;
pub const ERR_SYNTAX_ERROR: u8 = 1;
pub const ERR_OUT_OF_MEMORY: u8 = 2;
pub const ERR_STACK_OVERFLOW: u8 = 3;
pub const ERR_NOT_MATCH: u8 = 4;
pub const ERR_UNDEFINED_LINE: u8 = 5;
pub const ERR_DIVIDE_BY_ZERO: u8 = 6;
pub const ERR_INDEX_OUT_OF_RANGE: u8 = 7;
pub const ERR_FILE_ERROR: u8 = 8;
pub const ERR_SEGMENTATION_FAULT: u8 = 9;
pub const ERR_COMPLEX_EXPRESSION: u8 = 10;
pub const ERR_ILLEGAL_ARGUMENT: u8 = 11;
pub const ERR_BREAK: u8 = 12;

pub const ERR_MESSAGES: &[&str] = &[
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
