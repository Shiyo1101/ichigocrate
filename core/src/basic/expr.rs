// Copyright 2014-2024 the IchigoJam authors. All rights reserved. MIT license.
// https://github.com/IchigoJam/ichigojam-firm/blob/main/IchigoJam_BASIC/basic.h

//! 式評価。
//!
//! 再帰下降で式を評価する。各 `eval_*` は 1 つの優先順位の段に対応し、優先順位の
//! 低い段が高い段を呼ぶ形で連なる (低い段ほど後で結合する)。同じ段の演算子は
//! 左から順に評価する。優先順位は低い順に:
//!
//! 1. [`Machine::eval_logical_or`]      — `||`
//! 2. [`Machine::eval_logical_and`]     — `&&`
//! 3. [`Machine::eval_comparison`]      — `==` `!=` `<` `>` `<=` `>=`
//! 4. [`Machine::eval_additive`]        — `+` `-` とビット OR `|`
//! 5. [`Machine::eval_multiplicative`]  — `*` `/` `%` とビット `&` `^` `<<` `>>`
//! 6. [`Machine::eval_unary`]           — 単項 `-` `~` `!` と一次式 (数値・変数・関数・括弧)
//!
//! [`Machine::eval_expression`] が最下段から評価を始める入口。

use super::sin::sin360;
use super::{IJB_BUILD, IJB_VER, LANG_JP, VER_PLATFORM_PC};
use crate::errors::*;
use crate::machine::{calc_div, calc_mod, Machine};
use crate::ram::*;
use crate::tokens::*;

impl Machine {
    /// 式を 1 つ評価して値を返す、外部からの入口。最下段 (論理 OR) から評価する。
    pub fn eval_expression(&mut self) -> BResult<i16> {
        self.is_expr_mode = true;
        self.last_token_end_pc = 0;
        // is_expr_mode は途中でエラーになっても必ず false へ戻す必要があるため、
        // 本体を内部関数に分け、成否に関わらず後始末する。
        let result = self.eval_logical_or();
        self.is_expr_mode = false;
        self.last_token_end_pc = 0;
        result
    }

    /// 優先順位 1 (最低): 論理 OR `||`。0/非0 を真偽として畳み込む。
    fn eval_logical_or(&mut self) -> BResult<i16> {
        let mut value = self.eval_logical_and()?;
        loop {
            let t = self.token_get();
            if t.code != TOKEN_LOR_1 && t.code != TOKEN_LOR_2 {
                self.token_back();
                break;
            }
            let v2 = self.eval_logical_and()?;
            value = (value != 0 || v2 != 0) as i16;
        }
        Ok(value)
    }

    /// 優先順位 2: 論理 AND `&&`。0/非0 を真偽として畳み込む。
    fn eval_logical_and(&mut self) -> BResult<i16> {
        let mut value = self.eval_comparison()?;
        loop {
            let t = self.token_get();
            if t.code != TOKEN_LAND_1 && t.code != TOKEN_LAND_2 {
                self.token_back();
                break;
            }
            let v2 = self.eval_comparison()?;
            value = (value != 0 && v2 != 0) as i16;
        }
        Ok(value)
    }

    /// 優先順位 3: 比較 `==` `!=` `<` `>` `<=` `>=`。結果は 1 (真) / 0 (偽)。
    fn eval_comparison(&mut self) -> BResult<i16> {
        let mut value = self.eval_additive()?;
        loop {
            let t = self.token_get();
            if t.code < TOKEN_EQEQ || t.code > TOKEN_GT {
                self.token_back();
                break;
            }
            let rv = self.eval_additive()?;
            value = match t.code {
                TOKEN_GT => (value > rv) as i16,
                TOKEN_EQEQ | TOKEN_EQ => (value == rv) as i16,
                TOKEN_GE => (value >= rv) as i16,
                TOKEN_LT => (value < rv) as i16,
                TOKEN_NE_1 | TOKEN_NE_2 | TOKEN_NE_3 => (value != rv) as i16,
                TOKEN_LE => (value <= rv) as i16,
                _ => value,
            };
        }
        Ok(value)
    }

    /// 優先順位 4: 加減算 `+` `-` とビット OR `|` (本家 BASIC では同順位)。
    fn eval_additive(&mut self) -> BResult<i16> {
        let mut value = self.eval_multiplicative()?;
        loop {
            let t = self.token_get();
            if t.code < TOKEN_PLUS || t.code > TOKEN_OR {
                self.token_back();
                break;
            }
            let v2 = self.eval_multiplicative()?;
            value = match t.code {
                TOKEN_PLUS => value.wrapping_add(v2),
                TOKEN_MINUS => value.wrapping_sub(v2),
                _ => value | v2,
            };
        }
        Ok(value)
    }

    /// 優先順位 5: 乗除算 `*` `/` `%` とビット `&` `^` `<<` `>>` (本家 BASIC では同順位)。
    /// 0 除算は [`ERR_DIVIDE_BY_ZERO`]。
    fn eval_multiplicative(&mut self) -> BResult<i16> {
        let mut value = self.eval_unary()?;
        loop {
            let t = self.token_get();
            if t.code < TOKEN_AND || t.code > TOKEN_MOD_2 {
                self.token_back();
                break;
            }
            let v2 = self.eval_unary()?;
            match t.code {
                TOKEN_AND => value &= v2,
                TOKEN_XOR => value ^= v2,
                TOKEN_SHIFT_R => {
                    value = shift_signed(value, v2, true);
                }
                TOKEN_SHIFT_L => {
                    value = shift_signed(value, v2, false);
                }
                TOKEN_ASTER => value = value.wrapping_mul(v2),
                TOKEN_SLASH | TOKEN_MOD_1 | TOKEN_MOD_2 => {
                    if v2 == 0 {
                        return Err(ERR_DIVIDE_BY_ZERO);
                    }
                    if t.code == TOKEN_SLASH {
                        value = calc_div(value as i32, v2 as i32) as i16;
                    } else {
                        value = calc_mod(value as i32, v2 as i32) as i16;
                    }
                }
                _ => {}
            }
        }
        Ok(value)
    }

    /// 関数の必須引数 `(式)` を 1 つ評価し、閉じ括弧まで読む (例: `ABS(x)`)。
    fn eval_paren_arg(&mut self) -> BResult<i16> {
        let v = self.eval_expression()?;
        self.expect_paren_close()?;
        Ok(v)
    }

    /// 省略可能な引数。`()` のように空なら 0、`(式)` があればその値を返す
    /// (例: `BTN()` / `BTN(28)`)。
    fn eval_optional_arg(&mut self) -> BResult<i16> {
        let t = self.token_get();
        if t.code == TOKEN_PAREN_E {
            return Ok(0);
        }
        self.token_back();
        let v = self.eval_expression()?;
        self.expect_paren_close()?;
        Ok(v)
    }

    /// 優先順位 6 (最高): 単項演算子 (`-` 符号反転 / `~` ビット反転 / `!` 論理否定) と
    /// 一次式 (数値・変数・配列・括弧・各種組込み関数)。式の葉に当たる。
    fn eval_unary(&mut self) -> BResult<i16> {
        let t = self.token_get();
        match t.code {
            TOKEN_MINUS => Ok(self.eval_unary()?.wrapping_neg()),
            TOKEN_NOT => Ok(!self.eval_unary()?),
            TOKEN_LNOT_1 | TOKEN_LNOT_2 => Ok((self.eval_unary()? == 0) as i16),
            TOKEN_NUMBER => Ok(t.value),
            TOKEN_VAR => Ok(self.var_get(t.value as usize)),
            TOKEN_ARRAY => {
                let i = self.token_get_array_index()?;
                Ok(self.var_get(i))
            }
            TOKEN_PAREN_B => {
                let v = self.eval_expression()?;
                self.expect_paren_close()?;
                Ok(v)
            }
            TOKEN_INKEY => {
                self.expect_paren_close()?;
                let n = self.key_get_key();
                if n == 0 {
                    Ok(0x100)
                } else if n < 0 {
                    Ok(0)
                } else {
                    Ok(n as i16)
                }
            }
            TOKEN_BTN => {
                let n = self.eval_optional_arg()?;
                Ok(self.btn(n))
            }
            TOKEN_POS => {
                let n = self.eval_optional_arg()?;
                Ok(match n {
                    1 => self.cursorx as i16,
                    2 => self.cursory as i16,
                    3 => self.text_cols as i16,
                    4 => self.text_rows as i16,
                    _ => (self.cursorx + self.cursory * self.text_cols as i32) as i16,
                })
            }
            TOKEN_SOUND => {
                self.expect_paren_close()?;
                Ok(self.psg_sound() as i16)
            }
            TOKEN_ANA => {
                self.eval_optional_arg()?;
                Ok(0)
            }
            TOKEN_FREE => Ok(((IJB_SIZEOF_LIST as u16) - 2 - self.list_size) as i16),
            TOKEN_VER => {
                let n = self.eval_optional_arg()?;
                Ok(match n {
                    0 => (IJB_VER * 100 + IJB_BUILD) as i16,
                    3 => LANG_JP as i16,
                    4 => 60,
                    // VER(2) はキーボードレイアウト ID (0=US, 1=JA)。
                    // KBD コマンドで切替えた値を返す。
                    2 => self.keyboard_id as i16,
                    _ => VER_PLATFORM_PC as i16,
                })
            }
            TOKEN_LEN => {
                let n = self.eval_paren_arg()? as i32;
                if n >= OFFSET_RAMROM as i32 {
                    let mut p = (n - OFFSET_RAMROM as i32) as usize;
                    let mut cnt = 0i16;
                    while p < SIZE_RAM {
                        let c = self.ram[p];
                        if c == b'"' || c == 0 {
                            break;
                        }
                        p += 1;
                        cnt += 1;
                    }
                    Ok(cnt)
                } else {
                    Ok(0)
                }
            }
            TOKEN_TICK => {
                let n = self.eval_optional_arg()?;
                Ok(self.tick_count(n))
            }
            TOKEN_FILE => Ok(self.last_file_slot as i16),
            TOKEN_LINE => {
                let pc2 = if self.pc_in_list() { self.pc } else { self.break_resume_pc };
                if (OFFSET_RAM_LIST..OFFSET_RAM_LIST + SIZE_RAM_LIST).contains(&pc2) {
                    let mut index: u16 = 0;
                    loop {
                        let n = self.list_get_number(index);
                        let size = self.list_get_length(index) as usize;
                        if pc2 < OFFSET_RAM_LIST + index as usize + size + 4 {
                            return Ok(n);
                        }
                        if n == 0 {
                            break;
                        }
                        index = index.wrapping_add(size as u16).wrapping_add(4);
                    }
                }
                Ok(0)
            }
            TOKEN_LEFT | TOKEN_RIGHT | TOKEN_UP | TOKEN_DOWN | TOKEN_SPACE => {
                Ok(t.code as i16 - (TOKEN_LEFT as i16 - 28))
            }
            TOKEN_ABS => {
                let v = self.eval_paren_arg()?;
                Ok(v.unsigned_abs() as i16)
            }
            TOKEN_RND => {
                let n = self.eval_paren_arg()?;
                Ok(self.random(n))
            }
            TOKEN_PEEK_1 | TOKEN_PEEK_2 => {
                let v = self.eval_paren_arg()?;
                Ok(self.peek(v as i32) as i16)
            }
            TOKEN_SIN | TOKEN_COS => {
                let mut v = self.eval_paren_arg()?;
                if t.code == TOKEN_COS {
                    v += 90;
                }
                Ok(sin360(v as i32) as i16)
            }
            TOKEN_IN => {
                // 実機 GPIO 入力。デスクトップ移植では未対応のため 0 固定
                self.eval_optional_arg()?;
                Ok(0)
            }
            TOKEN_VPEEK | TOKEN_SCR | TOKEN_POINT => {
                let kind = t.code;
                let t = self.token_get();
                if t.code == TOKEN_PAREN_E {
                    return Ok(self.screen_get_current() as i16);
                }
                self.token_back();
                let v = self.eval_expression()?;
                self.expect_token(TOKEN_COMMA)?;
                let v2 = self.eval_expression()?;
                self.expect_paren_close()?;
                if kind == TOKEN_POINT {
                    Ok(self.screen_pset(v as i32, v2 as i32, 3) as i16)
                } else {
                    Ok(self.screen_get(v as i32, v2 as i32) as i16)
                }
            }
            TOKEN_USR => {
                self.eval_expression()?;
                let t = self.token_get();
                if t.code == TOKEN_COMMA {
                    self.eval_expression()?;
                    self.expect_paren_close()?;
                } else if t.code != TOKEN_PAREN_E {
                    return Err(ERR_SYNTAX_ERROR);
                }
                Ok(0)
            }
            TOKEN_STRING => {
                let p = self.skip_string_literal();
                Ok((p as i32 + OFFSET_RAMROM as i32) as i16)
            }
            TOKEN_AT => {
                let label_start = self.pc - 1;
                let mut index: u16 = 0;
                loop {
                    let num = self.list_get_number(index);
                    if num == 0 {
                        break;
                    }
                    let s_start = OFFSET_RAM_LIST + index as usize + 3;
                    // POKE で LIST が破壊された場合に行末まで走査できないことが
                    // あるため、LIST 領域内に収まらない位置は走査打ち切り。
                    if s_start >= OFFSET_RAM_LIST + SIZE_RAM_LIST {
                        break;
                    }
                    if self.ram[s_start] == b'@' {
                        let mut s = s_start;
                        let mut p = label_start;
                        let s_end = OFFSET_RAM_LIST + SIZE_RAM_LIST;
                        loop {
                            if s >= s_end {
                                break;
                            }
                            let c = self.ram[s];
                            s += 1;
                            if c == b':' || c == 0 || c == b'\'' || c == b' ' {
                                self.pc = p;
                                return Ok(num);
                            }
                            if c != self.ram_at(p) {
                                break;
                            }
                            p += 1;
                        }
                    }
                    index = index
                        .wrapping_add(self.list_get_length(index) as u16)
                        .wrapping_add(4);
                }
                Err(ERR_UNDEFINED_LINE)
            }
            _ => Err(ERR_SYNTAX_ERROR),
        }
    }
}

/// シフト量を i16 全域 (負値や ±16 超) で受けても panic せずに結果を返す。
/// シフト量の絶対値が 16 以上になる場合は IchigoJam の値域 (16bit) を
/// 越えるので 0 として扱う。`right=true` は SHIFT_R、`false` は SHIFT_L。
/// シフト方向は v2 の符号で反転する (例: SHIFT_R で v2<0 なら左シフト)。
fn shift_signed(value: i16, v2: i16, right: bool) -> i16 {
    let abs = (v2 as i32).unsigned_abs();
    if abs >= 16 {
        return 0;
    }
    let go_right = right == (v2 >= 0);
    if go_right {
        ((value as u16) >> abs) as i16
    } else {
        (value as u16).wrapping_shl(abs) as i16
    }
}
