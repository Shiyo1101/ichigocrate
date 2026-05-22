//! 式評価。

use super::sin::sin360;
use super::{IJB_VER, IJB_BUILD, LANG_JP, VER_PLATFORM_PC};
use crate::errors::*;
use crate::machine::{calc_div, calc_mod, Machine};
use crate::ram::*;
use crate::tokens::*;

impl Machine {
    pub fn token_expression(&mut self) -> i16 {
        self.tokenmode = 1;
        self.lasttokenpc = 0;
        let mut value = self.token_expression1();
        if self.err == 0 {
            loop {
                let t = self.token_get();
                if t.code != TOKEN_LOR_1 && t.code != TOKEN_LOR_2 {
                    self.token_back();
                    break;
                }
                let v2 = self.token_expression1();
                if self.err != 0 {
                    break;
                }
                value = if value != 0 || v2 != 0 { 1 } else { 0 };
            }
        }
        self.tokenmode = 0;
        self.lasttokenpc = 0;
        value
    }

    fn token_expression1(&mut self) -> i16 {
        let mut value = self.token_expression2();
        if self.err != 0 {
            return value;
        }
        loop {
            let t = self.token_get();
            if t.code != TOKEN_LAND_1 && t.code != TOKEN_LAND_2 {
                self.token_back();
                break;
            }
            let v2 = self.token_expression2();
            if self.err != 0 {
                break;
            }
            value = if value != 0 && v2 != 0 { 1 } else { 0 };
        }
        value
    }

    fn token_expression2(&mut self) -> i16 {
        let mut value = self.token_expression3();
        if self.err != 0 {
            return value;
        }
        loop {
            let t = self.token_get();
            if t.code < TOKEN_EQEQ || t.code > TOKEN_GT {
                self.token_back();
                break;
            }
            let rv = self.token_expression3();
            if self.err != 0 {
                break;
            }
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
        value
    }

    fn token_expression3(&mut self) -> i16 {
        let mut value = self.token_expression4();
        if self.err != 0 {
            return value;
        }
        loop {
            let t = self.token_get();
            if t.code < TOKEN_PLUS || t.code > TOKEN_OR {
                self.token_back();
                break;
            }
            let v2 = self.token_expression4();
            if self.err != 0 {
                break;
            }
            value = match t.code {
                TOKEN_PLUS => value.wrapping_add(v2),
                TOKEN_MINUS => value.wrapping_sub(v2),
                _ => value | v2,
            };
        }
        value
    }

    fn token_expression4(&mut self) -> i16 {
        let mut value = self.token_expression5();
        if self.err != 0 {
            return value;
        }
        loop {
            let t = self.token_get();
            if t.code < TOKEN_AND || t.code > TOKEN_MOD_2 {
                self.token_back();
                break;
            }
            let v2 = self.token_expression5();
            if self.err != 0 {
                break;
            }
            match t.code {
                TOKEN_AND => value &= v2,
                TOKEN_XOR => value ^= v2,
                TOKEN_SHIFT_R => {
                    if v2 > 0 {
                        value = ((value as u16) >> v2) as i16;
                    } else {
                        value = value.wrapping_shl((-v2) as u32);
                    }
                }
                TOKEN_SHIFT_L => {
                    if v2 > 0 {
                        value = value.wrapping_shl(v2 as u32);
                    } else {
                        value = ((value as u16) >> (-v2)) as i16;
                    }
                }
                TOKEN_ASTER => value = value.wrapping_mul(v2),
                TOKEN_SLASH | TOKEN_MOD_1 | TOKEN_MOD_2 => {
                    if v2 == 0 {
                        self.command_error(ERR_DIVIDE_BY_ZERO);
                        break;
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
        value
    }

    fn token_paren1(&mut self) -> i16 {
        let v = self.token_expression();
        if self.err != 0 {
            return v;
        }
        if !self.expect_paren_close() {
            return v;
        }
        v
    }

    fn token_opt1(&mut self) -> i16 {
        let t = self.token_get();
        if t.code == TOKEN_PAREN_E {
            return 0;
        }
        self.token_back();
        let v = self.token_expression();
        if self.err != 0 {
            return v;
        }
        let _ = self.expect_paren_close();
        v
    }

    fn token_expression5(&mut self) -> i16 {
        let t = self.token_get();
        match t.code {
            TOKEN_MINUS => -self.token_expression5(),
            TOKEN_NOT => !self.token_expression5(),
            TOKEN_LNOT_1 | TOKEN_LNOT_2 => {
                let v = self.token_expression5();
                if v == 0 {
                    1
                } else {
                    0
                }
            }
            TOKEN_NUMBER => t.value,
            TOKEN_VAR => self.var_get(t.value as usize),
            TOKEN_ARRAY => {
                let i = self.token_get_array_index();
                if self.err != 0 {
                    return 0;
                }
                self.var_get(i)
            }
            TOKEN_PAREN_B => {
                let v = self.token_expression();
                if self.err != 0 {
                    return 0;
                }
                if !self.expect_paren_close() {
                    return 0;
                }
                v
            }
            TOKEN_INKEY => {
                if !self.expect_paren_close() {
                    return 0;
                }
                let n = self.key_get_key();
                if n == 0 {
                    return 0x100;
                }
                if n < 0 {
                    return 0;
                }
                n as i16
            }
            TOKEN_BTN => {
                let _ = self.token_opt1();
                0
            }
            TOKEN_POS => {
                let n = self.token_opt1();
                match n {
                    1 => self.cursorx as i16,
                    2 => self.cursory as i16,
                    3 => self.screenw as i16,
                    4 => self.screenh as i16,
                    _ => (self.cursorx + self.cursory * self.screenw as i32) as i16,
                }
            }
            TOKEN_SOUND => {
                if !self.expect_paren_close() {
                    return 0;
                }
                if self.psg_sound() { 1 } else { 0 }
            }
            TOKEN_ANA => {
                let _ = self.token_opt1();
                0
            }
            TOKEN_FREE => ((IJB_SIZEOF_LIST as u16) - 2 - self.listsize) as i16,
            TOKEN_VER => {
                let n = self.token_opt1();
                match n {
                    0 => (IJB_VER * 100 + IJB_BUILD) as i16,
                    3 => LANG_JP as i16,
                    4 => 60,
                    2 => 0,
                    _ => VER_PLATFORM_PC as i16,
                }
            }
            TOKEN_LEN => {
                let n = self.token_paren1() as i32;
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
                    cnt
                } else {
                    0
                }
            }
            TOKEN_TICK => {
                let n = self.token_opt1();
                self.video_tick(n)
            }
            TOKEN_FILE => self.lastfile as i16,
            TOKEN_LINE => {
                let pc2 = if self.pc_in_list() { self.pc } else { self.pcbreak };
                if (OFFSET_RAM_LIST..OFFSET_RAM_LIST + SIZE_RAM_LIST).contains(&pc2) {
                    let mut index: u16 = 0;
                    loop {
                        let n = self.list_get_number(index);
                        let size = self.list_get_length(index) as usize;
                        if pc2 < OFFSET_RAM_LIST + index as usize + size + 4 {
                            return n;
                        }
                        if n == 0 {
                            break;
                        }
                        index = index.wrapping_add(size as u16).wrapping_add(4);
                    }
                }
                0
            }
            TOKEN_LEFT | TOKEN_RIGHT | TOKEN_UP | TOKEN_DOWN | TOKEN_SPACE => {
                t.code as i16 - (TOKEN_LEFT as i16 - 28)
            }
            TOKEN_ABS => {
                let v = self.token_paren1();
                v.unsigned_abs() as i16
            }
            TOKEN_RND => {
                let n = self.token_paren1();
                self.random(n)
            }
            TOKEN_PEEK_1 | TOKEN_PEEK_2 => {
                let v = self.token_paren1();
                self.peek(v as i32) as i16
            }
            TOKEN_SIN | TOKEN_COS => {
                let mut v = self.token_paren1();
                if t.code == TOKEN_COS {
                    v += 90;
                }
                sin360(v as i32) as i16
            }
            TOKEN_IN => {
                // 実機 GPIO 入力。デスクトップ移植では未対応のため 0 固定
                let _ = self.token_opt1();
                0
            }
            TOKEN_VPEEK | TOKEN_SCR | TOKEN_POINT => {
                let kind = t.code;
                let t = self.token_get();
                if t.code == TOKEN_PAREN_E {
                    return self.screen_get_current() as i16;
                }
                self.token_back();
                let v = self.token_expression();
                if self.err != 0 {
                    return 0;
                }
                if !self.expect_token(TOKEN_COMMA) {
                    return 0;
                }
                let v2 = self.token_expression();
                if self.err != 0 {
                    return 0;
                }
                if !self.expect_paren_close() {
                    return 0;
                }
                if kind == TOKEN_POINT {
                    self.screen_pset(v as i32, v2 as i32, 3) as i16
                } else {
                    self.screen_get(v as i32, v2 as i32) as i16
                }
            }
            TOKEN_USR => {
                let _v = self.token_expression();
                if self.err != 0 {
                    return 0;
                }
                let t = self.token_get();
                if t.code == TOKEN_COMMA {
                    let _ = self.token_expression();
                    if self.err != 0 {
                        return 0;
                    }
                    let _ = self.expect_paren_close();
                } else if t.code != TOKEN_PAREN_E {
                    self.command_error(ERR_SYNTAX_ERROR);
                    return 0;
                }
                0
            }
            TOKEN_STRING => {
                let p = self.token_skipstr();
                (p as i32 + OFFSET_RAMROM as i32) as i16
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
                    if self.ram[s_start] == b'@' {
                        let mut s = s_start;
                        let mut p = label_start;
                        loop {
                            let c = self.ram[s];
                            s += 1;
                            if c == b':' || c == 0 || c == b'\'' || c == b' ' {
                                self.pc = p;
                                return num;
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
                self.command_error(ERR_UNDEFINED_LINE);
                0
            }
            _ => {
                self.command_error(ERR_SYNTAX_ERROR);
                0
            }
        }
    }
}
