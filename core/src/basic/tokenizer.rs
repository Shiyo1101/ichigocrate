//! トークナイザと構文パーサ補助。
//!
//! `token_back` の 1 トークン戻し読みは `last_token_start_pc` / `last_token_end_pc` の
//! キャッシュで実現する。`expect_*` / `parse_*` は commands/expr 双方で
//! 共有される文/関数呼出パースの定型パターン。

use crate::errors::*;
use crate::machine::{basic_toupper, Machine, Token};
use crate::tokens::*;

impl Machine {
    /// `:` (文区切り) と EOF を 0 として返す。
    pub(super) fn token_get_char(&mut self) -> u8 {
        loop {
            if self.pc >= self.ram.len() {
                return 0;
            }
            let c = self.ram[self.pc];
            if c != b' ' {
                if c == b':' {
                    return 0;
                }
                return basic_toupper(c);
            }
            self.pc += 1;
        }
    }

    pub fn token_get(&mut self) -> Token {
        // 直前と同位置への問合せはキャッシュを返す (token_back の戻り対策)
        if self.pc == self.last_token_start_pc && self.last_token_end_pc != 0 {
            self.pc = self.last_token_end_pc;
            return self.last_token;
        }
        let mut tok = Token::default();
        let c = self.token_get_char();
        self.last_token_start_pc = self.pc;
        if c == 0 {
            tok.code = TOKEN_NULL;
        } else if c.is_ascii_digit() {
            tok.code = TOKEN_NUMBER;
            let mut c = c;
            loop {
                tok.value = tok.value.wrapping_mul(10).wrapping_add((c - b'0') as i16);
                self.pc += 1;
                c = self.token_get_char();
                if !c.is_ascii_digit() {
                    break;
                }
            }
        } else if c == b'#' {
            self.pc += 1;
            let mut c = self.token_get_char();
            if !(c.is_ascii_digit() || (b'A'..=b'F').contains(&c)) {
                tok.code = TOKEN_ERROR;
            } else {
                tok.code = TOKEN_NUMBER;
                let mut value: i32 = 0;
                loop {
                    let n = if c <= b'9' { c - b'0' } else { c - b'A' + 10 };
                    value = (value << 4) + n as i32;
                    self.pc += 1;
                    c = self.token_get_char();
                    if c == b'L' || c == b'N' {
                        self.pc -= 1;
                        value >>= 4;
                        break;
                    }
                    if !(c.is_ascii_digit() || (b'A'..=b'F').contains(&c)) {
                        break;
                    }
                }
                tok.value = (value & 0xffff) as i16;
            }
        } else if c == b'`' {
            self.pc += 1;
            let mut c = self.token_get_char();
            if c != b'0' && c != b'1' {
                tok.code = TOKEN_ERROR;
            } else {
                tok.code = TOKEN_NUMBER;
                loop {
                    tok.value = (tok.value << 1) + (c - b'0') as i16;
                    self.pc += 1;
                    c = self.token_get_char();
                    if c != b'0' && c != b'1' {
                        break;
                    }
                }
            }
        } else {
            // トークンテーブル検索 (式モードでは予約語の一部のみマッチ)
            let max = if self.is_expr_mode {
                N_TOKEN_EXPRESSION as usize
            } else {
                N_TOKEN
            };
            let mut p = 0usize;
            let mut matched = None;
            for i in 0..max {
                let len = TOKENS[p] as usize;
                let mut hit = true;
                for j in 1..len {
                    let mut c2;
                    loop {
                        c2 = if self.pc < self.ram.len() {
                            self.ram[self.pc]
                        } else {
                            0
                        };
                        self.pc += 1;
                        if c2 != b' ' {
                            break;
                        }
                    }
                    if basic_toupper(c2) != TOKENS[p + j] {
                        hit = false;
                        break;
                    }
                }
                if hit {
                    matched = Some(i);
                    break;
                }
                p += len;
                self.pc = self.last_token_start_pc;
            }
            if let Some(i) = matched {
                tok.code = i as u16 + N_TOKEN_OFFSET;
            } else if c.is_ascii_uppercase() {
                self.pc += 1;
                tok.code = TOKEN_VAR;
                tok.value = (c - b'A' + crate::ram::IJB_SIZEOF_ARRAY as u8) as i16;
            } else {
                self.pc += 1;
                tok.code = TOKEN_ERROR;
            }
        }
        self.last_token = tok;
        self.last_token_end_pc = self.pc;
        tok
    }

    /// 1 トークンの先読み専用 (連続呼出は不可)。
    pub fn token_back(&mut self) {
        self.pc = self.last_token_start_pc;
    }

    // 以下のヘルパは `BResult` を返し、エラーは `Err(BasicError)` として
    // 呼出元へ `?` で伝搬する契約で統一されている。

    pub(super) fn expect_token(&mut self, code: u16) -> BResult<()> {
        if self.token_get().code == code {
            Ok(())
        } else {
            Err(ERR_SYNTAX_ERROR)
        }
    }

    pub(super) fn expect_paren_close(&mut self) -> BResult<()> {
        self.expect_token(TOKEN_PAREN_E)
    }

    /// `VAR` または `ARRAY` を読んで変数領域内のインデックスを返す。
    pub(super) fn parse_lvalue_index(&mut self) -> BResult<usize> {
        let t = self.token_get();
        match t.code {
            TOKEN_VAR => Ok(t.value as usize),
            TOKEN_ARRAY => self.token_get_array_index(),
            _ => Err(ERR_SYNTAX_ERROR),
        }
    }

    /// 行末 (TOKEN_NULL / TOKEN_ELSE) なら `default`、そうでなければ式を 1 つ読む。
    pub(super) fn parse_optional_expr(&mut self, default: i16) -> BResult<i16> {
        let code = self.token_get().code;
        self.token_back();
        if code == TOKEN_NULL || code == TOKEN_ELSE {
            return Ok(default);
        }
        self.eval_expression()
    }

    /// `HEX$/BIN$/DEC$/STR$` のような `expr` または `expr,m` + `)` をパース。
    pub(super) fn parse_format_args(&mut self, default_m: i16) -> BResult<(i16, i16)> {
        let n = self.eval_expression()?;
        let t = self.token_get();
        let m = if t.code == TOKEN_COMMA {
            let m = self.eval_expression()?;
            self.expect_paren_close()?;
            m
        } else if t.code == TOKEN_PAREN_E {
            default_m
        } else {
            return Err(ERR_SYNTAX_ERROR);
        };
        Ok((n, m))
    }

    pub(super) fn token_get_array_index(&mut self) -> BResult<usize> {
        let v = self.eval_expression()?;
        if self.token_get().code != TOKEN_ARRAY_E {
            return Err(ERR_SYNTAX_ERROR);
        }
        if v < 0 || v as usize >= crate::ram::IJB_SIZEOF_ARRAY {
            return Err(ERR_INDEX_OUT_OF_RANGE);
        }
        Ok(v as usize)
    }

    /// `TOKEN_NULL` か `TOKEN_ELSE` 以外なら Syntax error。
    pub(super) fn expect_statement_end(&mut self) -> BResult<()> {
        let code = self.token_get().code;
        self.token_back();
        if code != TOKEN_NULL && code != TOKEN_ELSE {
            return Err(ERR_SYNTAX_ERROR);
        }
        Ok(())
    }

    /// 文字列リテラル本体を画面に流して終端の `"` を消費する。
    pub(super) fn print_string_literal(&mut self) {
        while self.pc < self.ram.len() && self.ram[self.pc] != 0 && self.ram[self.pc] != b'"' {
            let c = self.ram[self.pc];
            self.put_chr(c);
            self.pc += 1;
        }
        if self.pc < self.ram.len() && self.ram[self.pc] == b'"' {
            self.pc += 1;
        }
    }

    /// 文字列リテラルを読み飛ばし、本体先頭の RAM インデックスを返す。
    pub(super) fn skip_string_literal(&mut self) -> usize {
        let res = self.pc;
        while self.pc < self.ram.len() && self.ram[self.pc] != 0 && self.ram[self.pc] != b'"' {
            self.pc += 1;
        }
        if self.pc < self.ram.len() && self.ram[self.pc] == b'"' {
            self.pc += 1;
        }
        res
    }

    /// 省略可能な `,expr` を読んでから文末を確認する (WAIT / RENUM 等)。
    pub(super) fn parse_optional_trailing_arg(&mut self, default_value: i16) -> BResult<i16> {
        let code = self.token_get().code;
        if code != TOKEN_COMMA {
            self.token_back();
            self.expect_statement_end()?;
            Ok(default_value)
        } else {
            let v = self.eval_expression()?;
            self.expect_statement_end()?;
            Ok(v)
        }
    }
}
