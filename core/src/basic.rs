//! basic.h を Rust に移植。BASIC インタプリタ (トークナイザ + 評価器 + 文実行)。

use crate::errors::*;
use crate::machine::{basic_toupper, calc_div, calc_mod, strlen8, BasicResult, Machine, Token, PC_NULL};
use crate::ram::*;
use crate::tokens::*;

const IJB_VER: i32 = 143;
const IJB_BUILD: i32 = 28;
const LANG_JP: i32 = 1;
const VER_PLATFORM_PC: i32 = 4;

impl Machine {
    // ============================================================
    // メインループ (REPL から呼び出される)
    // ============================================================

    /// BASIC 実行を開始する。`commandline_pc` は RAM インデックス。
    pub fn basic_start(&mut self, commandline_pc: usize) {
        self.err = 0;
        self.ngosubstack = 0;
        self.nforstack = 0;
        self.tokenmode = 0;
        self.pc = commandline_pc;
        self.lasttoken = 0;
        self.lasttokenpc = 0;
    }

    /// 1 文ぶんだけ実行する。返り値が `Some` なら停止 (理由付き)、`None`
    /// なら継続。協調的実行のため egui アプリは毎フレーム複数回呼ぶ。
    pub fn basic_step(&mut self) -> Option<BasicResult> {
        if self.pc == PC_NULL {
            return Some(BasicResult::Execute);
        }
        self.token_get_char();
        if self.pc == PC_NULL {
            return Some(BasicResult::Execute);
        }
        let c = self.ram_at(self.pc);
        if c == b':' {
            self.pc += 1;
            return None;
        } else if c == b'\'' {
            self.command_rem();
            return None;
        } else if c == 0 {
            // LIST 領域では、ステートメントは偶数バイトに揃えられ、奇数位置に
            // 終端 NULL がある。偶数 PC で NULL に当たった場合 (= 統計詰めの
            // パディング NULL) は +1 して実際の終端へ進める (C 版 AddrIsOdd 相当)。
            if self.pc >= OFFSET_RAM_LIST
                && self.pc < OFFSET_RAM_LIST + SIZE_RAM_LIST
                && (self.pc & 1) == (OFFSET_RAM_LIST & 1)
            {
                self.pc += 1;
            }
            if self.pc >= OFFSET_RAM_LIST
                && self.pc + 4 < OFFSET_RAM_LIST + self.listsize as usize
            {
                self.pc += 4;
                return None;
            }
            return Some(BasicResult::Execute);
        }

        let token = self.token_get();
        match token.code {
            TOKEN_NULL => {}
            TOKEN_NUMBER => {
                self.command_edit(token.value);
                self.pc = PC_NULL;
                return Some(BasicResult::Edit);
            }
            TOKEN_VAR | TOKEN_ARRAY => {
                self.token_back();
                self.command_let(TOKEN_EQ);
            }
            TOKEN_AT => self.command_at(),
            TOKEN_IF => self.command_if(),
            TOKEN_ELSE => self.command_rem(),
            TOKEN_FOR => self.command_for(),
            TOKEN_NEXT => self.command_next(),
            TOKEN_GOTO => self.command_goto(),
            TOKEN_GOSUB_1 | TOKEN_GOSUB_2 => self.command_gosub(),
            TOKEN_RETURN_1 | TOKEN_RETURN_2 => self.command_return(),
            TOKEN_END | TOKEN_STOP => self.command_end(),
            TOKEN_REM_1 | TOKEN_REM_2 => self.command_rem(),
            TOKEN_CONT => self.command_cont(),
            TOKEN_OK => self.command_ok(),
            TOKEN_NEW => self.command_new(),
            TOKEN_RUN => self.command_run(),
            TOKEN_LET => self.command_let(TOKEN_COMMA),
            TOKEN_CLS => self.command_cls(),
            TOKEN_LOCATE_1 | TOKEN_LOCATE_2 => self.command_locate(),
            TOKEN_PRINT_1 | TOKEN_PRINT_2 => self.command_print(),
            TOKEN_INPUT => self.command_input(),
            TOKEN_CLV_1 | TOKEN_CLV_2 => self.command_clv(),
            TOKEN_CLK => self.command_clk(),
            TOKEN_SRND => self.command_srnd(),
            TOKEN_DRAW => self.command_draw(),
            TOKEN_WAIT => self.command_wait(),
            TOKEN_CLT => self.command_clt(),
            TOKEN_OUT => self.command_out(),
            TOKEN_LED => self.command_led(),
            TOKEN_CLO => self.command_clo(),
            TOKEN_RENUM => self.command_renum(),
            TOKEN_SCROLL => self.command_scroll(),
            TOKEN_BEEP => self.command_beep(),
            TOKEN_TEMPO => self.command_tempo(),
            TOKEN_PLAY => self.command_play(),
            TOKEN_POKE => self.command_poke(),
            TOKEN_COPY => self.command_copy(),
            TOKEN_CLP => self.command_clp(),
            TOKEN_LIST => self.command_list(),
            TOKEN_LOAD => self.command_load(false),
            TOKEN_LRUN => self.command_load(true),
            TOKEN_SAVE => self.command_save(),
            TOKEN_FILES => self.command_files(),
            TOKEN_HELP => self.command_help(),
            _ => {
                self.command_error(ERR_SYNTAX_ERROR);
            }
        }

        if self.err != 0 {
            return Some(BasicResult::StopOrErr);
        }
        if self.stop_execute() {
            self.command_error(ERR_BREAK);
            return Some(BasicResult::StopOrErr);
        }
        None
    }

    /// 1 行ぶんを実行する。RUN/GOTO 等で PC が LIST 領域へ移動した場合は
    /// 呼出元 (UI アプリ) へ制御を返し、以降は `basic_step` を毎フレーム
    /// 呼び出してプログラムを進める。
    pub fn basic_execute(&mut self, commandline_pc: usize) -> BasicResult {
        self.basic_start(commandline_pc);
        let started_in_list = commandline_pc >= OFFSET_RAM_LIST
            && commandline_pc < OFFSET_RAM_LIST + SIZE_RAM_LIST;
        loop {
            if let Some(r) = self.basic_step() {
                return r;
            }
            // WAIT で待機要求があれば呼出元へ
            if self.wait_frames > 0 {
                return BasicResult::Execute;
            }
            // 即時入力 → プログラム実行への遷移を検知
            if !started_in_list {
                let in_list = self.pc >= OFFSET_RAM_LIST
                    && self.pc < OFFSET_RAM_LIST + SIZE_RAM_LIST;
                if in_list {
                    return BasicResult::Execute;
                }
            }
        }
    }

    fn ram_at(&self, pc: usize) -> u8 {
        if pc < self.ram.len() {
            self.ram[pc]
        } else {
            0
        }
    }

    // ============================================================
    // command_edit: 行番号付き入力時のプログラム編集
    // ============================================================

    fn command_edit(&mut self, number: i16) {
        // 実行中 (pc が LIST 内) であれば文法エラー
        if number <= 0
            || (self.pc >= OFFSET_RAM_LIST && self.pc < OFFSET_RAM_LIST + IJB_SIZEOF_LIST)
        {
            self.command_error(ERR_SYNTAX_ERROR);
            return;
        }
        let found = self.list_find(number);
        if self.list_get_number(found) == number {
            // 既存行を削除
            let len = self.list_get_length(found) as u16 + 4;
            let mut dst = found as usize;
            let mut src = found as usize + len as usize;
            while src < self.listsize as usize {
                self.ram[OFFSET_RAM_LIST + dst] = self.ram[OFFSET_RAM_LIST + src];
                dst += 1;
                src += 1;
            }
            self.listsize -= len;
            self.list_set_number(self.listsize, 0);
        }

        // 末尾スペース除去
        while self.pc > 0 && self.ram_at(self.pc - 1) == b' ' {
            if self.pc < 2 || self.ram_at(self.pc - 2) != b' ' {
                break;
            }
            self.pc -= 1;
        }
        if self.ram_at(self.pc) == 0 {
            return; // 行番号のみ → 削除のみで終了
        }
        let len_str = strlen8(&self.ram, self.pc);
        let align = (len_str & 1) as u16;
        let mut src = self.listsize as i32;
        let dst_end = self.listsize + len_str as u16 + align + 4;
        if dst_end as usize + 2 > IJB_SIZEOF_LIST {
            self.command_error(ERR_OUT_OF_MEMORY);
            return;
        }
        self.listsize = dst_end;
        let mut dst = dst_end as i32;
        while src > found as i32 {
            dst -= 1;
            src -= 1;
            self.ram[OFFSET_RAM_LIST + dst as usize] =
                self.ram[OFFSET_RAM_LIST + src as usize];
        }
        self.list_set_number(self.listsize, 0);
        self.list_set_number(found, number);
        self.list_set_length(found, len_str as u8);
        let mut dst = found as usize + 3;
        loop {
            let c = self.ram_at(self.pc);
            self.pc += 1;
            self.ram[OFFSET_RAM_LIST + dst] = c;
            dst += 1;
            if c == 0 {
                break;
            }
        }
        if align == 1 {
            self.ram[OFFSET_RAM_LIST + dst] = 0;
        }
    }

    // ============================================================
    // トークナイザ
    // ============================================================

    fn token_get_char(&mut self) -> u8 {
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
        // キャッシュヒット
        if self.pc == self.lasttoken && self.lasttokenpc != 0 {
            self.pc = self.lasttokenpc;
            return self.bklasttoken;
        }
        let mut tok = Token::default();
        let c = self.token_get_char();
        self.lasttoken = self.pc;
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
            // トークンテーブル検索
            let max = if self.tokenmode != 0 {
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
                self.pc = self.lasttoken;
            }
            if let Some(i) = matched {
                tok.code = i as u16 + N_TOKEN_OFFSET;
            } else if (b'A'..=b'Z').contains(&c) {
                self.pc += 1;
                tok.code = TOKEN_VAR;
                tok.value = (c - b'A' + IJB_SIZEOF_ARRAY as u8) as i16;
            } else {
                self.pc += 1;
                tok.code = TOKEN_ERROR;
            }
        }
        self.bklasttoken = tok;
        self.lasttokenpc = self.pc;
        tok
    }

    pub fn token_back(&mut self) {
        self.pc = self.lasttoken;
    }

    fn token_get_array_index(&mut self) -> usize {
        let v = self.token_expression();
        if self.err != 0 {
            return 0;
        }
        let t = self.token_get();
        if t.code != TOKEN_ARRAY_E {
            self.command_error(ERR_SYNTAX_ERROR);
            return 0;
        }
        if v < 0 || v as usize >= IJB_SIZEOF_ARRAY {
            self.command_error(ERR_INDEX_OUT_OF_RANGE);
            return 0;
        }
        v as usize
    }

    fn token_end(&mut self) {
        let code = self.token_get().code;
        self.token_back();
        if code != TOKEN_NULL && code != TOKEN_ELSE {
            self.command_error(ERR_SYNTAX_ERROR);
        }
    }

    fn token_puts(&mut self) {
        while self.pc < self.ram.len() && self.ram[self.pc] != 0 && self.ram[self.pc] != b'"' {
            let c = self.ram[self.pc];
            self.put_chr(c);
            self.pc += 1;
        }
        if self.pc < self.ram.len() && self.ram[self.pc] == b'"' {
            self.pc += 1;
        }
    }

    /// 文字列を読み飛ばし、先頭の RAM インデックスを返す
    fn token_skipstr(&mut self) -> usize {
        let res = self.pc;
        while self.pc < self.ram.len() && self.ram[self.pc] != 0 && self.ram[self.pc] != b'"' {
            self.pc += 1;
        }
        if self.pc < self.ram.len() && self.ram[self.pc] == b'"' {
            self.pc += 1;
        }
        res
    }

    // ============================================================
    // 式評価 (優先順位毎)
    // ============================================================

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
        let t = self.token_get();
        if t.code != TOKEN_PAREN_E {
            self.command_error(ERR_SYNTAX_ERROR);
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
        let t = self.token_get();
        if t.code != TOKEN_PAREN_E {
            self.command_error(ERR_SYNTAX_ERROR);
        }
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
                let t = self.token_get();
                if t.code != TOKEN_PAREN_E {
                    self.command_error(ERR_SYNTAX_ERROR);
                    return 0;
                }
                v
            }
            TOKEN_INKEY => {
                let t = self.token_get();
                if t.code != TOKEN_PAREN_E {
                    self.command_error(ERR_SYNTAX_ERROR);
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
                let t = self.token_get();
                if t.code != TOKEN_PAREN_E {
                    self.command_error(ERR_SYNTAX_ERROR);
                    return 0;
                }
                if self.psg_sound() {
                    1
                } else {
                    0
                }
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
                let pc2 = if self.pc >= OFFSET_RAM_LIST && self.pc < OFFSET_RAM_LIST + SIZE_RAM_LIST
                {
                    self.pc
                } else {
                    self.pcbreak
                };
                if pc2 >= OFFSET_RAM_LIST && pc2 < OFFSET_RAM_LIST + SIZE_RAM_LIST {
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
                let v = self.token_opt1();
                if v == 0 {
                    0
                } else {
                    0
                }
            }
            TOKEN_VPEEK | TOKEN_SCR | TOKEN_POINT => {
                let type_ = t.code;
                let t = self.token_get();
                if t.code == TOKEN_PAREN_E {
                    return self.screen_get_current() as i16;
                }
                self.token_back();
                let v = self.token_expression();
                if self.err != 0 {
                    return 0;
                }
                let t = self.token_get();
                if t.code != TOKEN_COMMA {
                    self.command_error(ERR_SYNTAX_ERROR);
                    return 0;
                }
                let v2 = self.token_expression();
                if self.err != 0 {
                    return 0;
                }
                let t = self.token_get();
                if t.code != TOKEN_PAREN_E {
                    self.command_error(ERR_SYNTAX_ERROR);
                    return 0;
                }
                if type_ == TOKEN_POINT {
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
                    let t = self.token_get();
                    if t.code != TOKEN_PAREN_E {
                        self.command_error(ERR_SYNTAX_ERROR);
                        return 0;
                    }
                } else if t.code != TOKEN_PAREN_E {
                    self.command_error(ERR_SYNTAX_ERROR);
                    return 0;
                }
                0
            }
            TOKEN_STRING => {
                // RAM インデックス p を仮想アドレスに変換 (p + OFFSET_RAMROM)
                let p = self.token_skipstr();
                (p as i32 + OFFSET_RAMROM as i32 - 0) as i16
            }
            TOKEN_AT => {
                // ラベル探索
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

    // ============================================================
    // 文 (command_*)
    // ============================================================

    fn command_rem(&mut self) {
        while self.pc < self.ram.len() && self.ram[self.pc] != 0 {
            self.pc += 1;
        }
    }

    fn command_let(&mut self, terminator: u16) {
        let t = self.token_get();
        let mut v: usize;
        match t.code {
            TOKEN_VAR => v = t.value as usize,
            TOKEN_ARRAY => {
                v = self.token_get_array_index();
                if self.err != 0 {
                    return;
                }
                // 配列 + COMMA で連続代入
                if terminator == TOKEN_COMMA {
                    let t = self.token_get();
                    if t.code != terminator {
                        self.command_error(ERR_SYNTAX_ERROR);
                        return;
                    }
                    self.command_let2(v);
                    loop {
                        if self.err != 0 {
                            return;
                        }
                        let t = self.token_get();
                        if t.code != terminator {
                            self.token_back();
                            self.token_end();
                            return;
                        }
                        v += 1;
                        if v >= IJB_SIZEOF_ARRAY {
                            self.command_error(ERR_INDEX_OUT_OF_RANGE);
                            return;
                        }
                        self.command_let2(v);
                    }
                }
            }
            _ => {
                self.command_error(ERR_SYNTAX_ERROR);
                return;
            }
        }
        let t = self.token_get();
        if t.code != terminator {
            self.command_error(ERR_SYNTAX_ERROR);
            return;
        }
        self.command_let2(v);
        if self.err != 0 {
            return;
        }
        self.token_end();
    }

    fn command_let2(&mut self, v: usize) -> i16 {
        let value = self.token_expression();
        if self.err != 0 {
            return 0;
        }
        self.var_set(v, value);
        value
    }

    fn command_if(&mut self) {
        let b = self.token_expression();
        if self.err != 0 {
            return;
        }
        if b != 0 {
            let t = self.token_get();
            if t.code != TOKEN_THEN {
                self.token_back();
            }
        } else {
            loop {
                let code = self.token_get().code;
                if code == TOKEN_NULL {
                    if self.ram_at(self.pc) == 0 {
                        break;
                    }
                    self.pc += 1;
                } else if code == TOKEN_STRING {
                    self.token_skipstr();
                } else if code == TOKEN_ELSE {
                    break;
                } else if code == TOKEN_IF || code == TOKEN_REM_1 || code == TOKEN_REM_2 {
                    self.command_rem();
                    break;
                }
            }
        }
    }

    fn command_for(&mut self) {
        if self.nforstack as usize >= IJB_SIZEOF_FOR_STACK {
            self.command_error(ERR_STACK_OVERFLOW);
            return;
        }
        self.forstack[self.nforstack as usize] = self.pc;
        self.nforstack += 1;

        let t = self.token_get();
        let v: usize = match t.code {
            TOKEN_VAR => t.value as usize,
            TOKEN_ARRAY => {
                let i = self.token_get_array_index();
                if self.err != 0 {
                    return;
                }
                i
            }
            _ => {
                self.command_error(ERR_SYNTAX_ERROR);
                return;
            }
        };
        let t = self.token_get();
        if t.code != TOKEN_EQ && t.code != TOKEN_COMMA {
            self.command_error(ERR_SYNTAX_ERROR);
            return;
        }
        let ival = self.command_let2(v);
        if self.err != 0 {
            return;
        }
        let t = self.token_get();
        if t.code != TOKEN_TO {
            self.command_error(ERR_SYNTAX_ERROR);
            return;
        }
        let to = self.token_expression();
        if self.err != 0 {
            return;
        }
        let mut step: i16 = 1;
        let t = self.token_get();
        if t.code != TOKEN_STEP {
            self.token_back();
        } else {
            step = self.token_expression();
            if self.err != 0 {
                return;
            }
        }
        if (step > 0 && ival > to) || (step < 0 && ival < to) {
            self.command_error(ERR_ILLEGAL_ARGUMENT);
            return;
        }
        self.token_end();
    }

    fn command_next(&mut self) {
        if self.nforstack == 0 {
            self.command_error(ERR_NOT_MATCH);
            return;
        }
        self.token_end();
        let bkpc = self.pc;
        self.pc = self.forstack[self.nforstack as usize - 1];
        let t = self.token_get();
        let v: usize = match t.code {
            TOKEN_VAR => t.value as usize,
            TOKEN_ARRAY => {
                let i = self.token_get_array_index();
                if self.err != 0 {
                    return;
                }
                i
            }
            _ => {
                self.command_error(ERR_SYNTAX_ERROR);
                return;
            }
        };
        let t = self.token_get();
        if t.code != TOKEN_EQ && t.code != TOKEN_COMMA {
            self.command_error(ERR_SYNTAX_ERROR);
            return;
        }
        let ival = self.token_expression();
        if self.err != 0 {
            return;
        }
        let t = self.token_get();
        if t.code != TOKEN_TO {
            self.command_error(ERR_SYNTAX_ERROR);
            return;
        }
        let to = self.token_expression();
        if self.err != 0 {
            return;
        }
        let mut step: i16 = 1;
        let t = self.token_get();
        if t.code != TOKEN_STEP {
            self.token_back();
        } else {
            step = self.token_expression();
            if self.err != 0 {
                return;
            }
        }
        self.token_end();

        let cur = self.var_get(v);
        if cur == to {
            self.pc = bkpc;
            self.nforstack -= 1;
            return;
        }
        if ival <= to {
            if cur.wrapping_add(step) > to {
                self.pc = bkpc;
                self.nforstack -= 1;
                return;
            }
        } else if cur.wrapping_add(step) < to {
            self.pc = bkpc;
            self.nforstack -= 1;
            return;
        }
        self.var_set(v, cur.wrapping_add(step));
    }

    fn command_goto(&mut self) {
        let n = self.token_expression();
        if self.err != 0 {
            return;
        }
        let idx = self.list_find_goto(n);
        if idx < 0 {
            self.command_error(ERR_UNDEFINED_LINE);
            return;
        }
        self.token_end();
        self.list_set_pc(idx as u16);
    }

    fn command_gosub(&mut self) {
        if self.ngosubstack as usize >= IJB_SIZEOF_GOSUB_STACK {
            self.command_error(ERR_STACK_OVERFLOW);
            return;
        }
        let n = self.token_expression();
        if self.err != 0 {
            return;
        }
        let idx = self.list_find_goto(n);
        if idx < 0 {
            self.command_error(ERR_UNDEFINED_LINE);
            return;
        }
        self.token_end();
        self.gosubstack[self.ngosubstack as usize] = self.pc;
        self.ngosubstack += 1;
        self.list_set_pc(idx as u16);
    }

    fn command_return(&mut self) {
        if self.ngosubstack == 0 {
            self.command_error(ERR_NOT_MATCH);
            return;
        }
        self.token_end();
        self.ngosubstack -= 1;
        self.pc = self.gosubstack[self.ngosubstack as usize];
    }

    fn command_cont(&mut self) {
        self.token_end();
        if self.pc < OFFSET_RAM_LIST || self.pc >= OFFSET_RAM_LIST + 1026 {
            self.pc = self.pcbreak;
        }
        if self.pc >= OFFSET_RAM_LIST && self.pc < OFFSET_RAM_LIST + SIZE_RAM_LIST {
            let mut index: u16 = 0;
            loop {
                let n = self.list_get_number(index);
                let size = self.list_get_length(index) as usize;
                if self.pc < OFFSET_RAM_LIST + index as usize + size + 4 {
                    let i = self.list_find_goto(n);
                    if i < 0 {
                        self.command_error(ERR_UNDEFINED_LINE);
                        return;
                    }
                    self.list_set_pc(i as u16);
                    break;
                }
                if n == 0 {
                    break;
                }
                index = index.wrapping_add(size as u16).wrapping_add(4);
            }
        }
    }

    fn command_print(&mut self) {
        let mut retflg = true;
        loop {
            let t = self.token_get();
            if t.code == TOKEN_NULL || t.code == TOKEN_ELSE {
                self.token_back();
                break;
            }
            match t.code {
                TOKEN_STRING => self.token_puts(),
                TOKEN_CHR => loop {
                    let n = self.token_expression();
                    if self.err != 0 {
                        return;
                    }
                    self.put_chr((n & 0xff) as u8);
                    let t = self.token_get();
                    if t.code == TOKEN_COMMA {
                        continue;
                    }
                    if t.code != TOKEN_PAREN_E {
                        self.command_error(ERR_SYNTAX_ERROR);
                        return;
                    }
                    break;
                },
                TOKEN_DEC => {
                    let n2 = self.token_expression();
                    if self.err != 0 {
                        return;
                    }
                    let mut m: i16 = 0;
                    let t = self.token_get();
                    if t.code == TOKEN_COMMA {
                        m = self.token_expression();
                        if self.err != 0 {
                            return;
                        }
                        let t = self.token_get();
                        if t.code != TOKEN_PAREN_E {
                            self.command_error(ERR_SYNTAX_ERROR);
                            return;
                        }
                    } else if t.code != TOKEN_PAREN_E {
                        self.command_error(ERR_SYNTAX_ERROR);
                        return;
                    }
                    if m <= 0 {
                        self.put_num(n2 as i32);
                    } else {
                        let beam = Machine::beam(n2 as i32);
                        if beam as i16 <= m {
                            for _ in 0..(m as u32 - beam) {
                                self.put_chr(b' ');
                            }
                            self.put_num(n2 as i32);
                        } else {
                            let mut n2 = n2 as i32;
                            if n2 < 0 {
                                n2 = -n2;
                            }
                            let mut beam = 5i32;
                            let mut d: u32 = 10000;
                            while d > 0 {
                                let c = (n2 as u32) / d;
                                if beam <= m as i32 {
                                    self.put_chr(c as u8 + b'0');
                                }
                                n2 -= (c * d) as i32;
                                beam -= 1;
                                d /= 10;
                            }
                        }
                    }
                }
                TOKEN_HEX => {
                    let n2 = (self.token_expression() as u16) & 0xffff;
                    if self.err != 0 {
                        return;
                    }
                    let mut m: i16 = 0;
                    let t = self.token_get();
                    if t.code == TOKEN_COMMA {
                        m = self.token_expression();
                        if self.err != 0 {
                            return;
                        }
                        let t = self.token_get();
                        if t.code != TOKEN_PAREN_E {
                            self.command_error(ERR_SYNTAX_ERROR);
                            return;
                        }
                    } else if t.code != TOKEN_PAREN_E {
                        self.command_error(ERR_SYNTAX_ERROR);
                        return;
                    }
                    if m == 0 {
                        let mut n3 = n2;
                        loop {
                            m += 1;
                            n3 >>= 4;
                            if n3 == 0 {
                                break;
                            }
                        }
                    }
                    for i in (0..m).rev() {
                        let h = (n2 >> (i * 4)) & 0xf;
                        if h >= 10 {
                            self.put_chr((h as u8) + b'A' - 10);
                        } else {
                            self.put_chr((h as u8) + b'0');
                        }
                    }
                }
                TOKEN_BIN => {
                    let n2 = (self.token_expression() as u16) & 0xffff;
                    if self.err != 0 {
                        return;
                    }
                    let mut m: i16 = 0;
                    let t = self.token_get();
                    if t.code == TOKEN_COMMA {
                        m = self.token_expression();
                        if self.err != 0 {
                            return;
                        }
                        let t = self.token_get();
                        if t.code != TOKEN_PAREN_E {
                            self.command_error(ERR_SYNTAX_ERROR);
                            return;
                        }
                    } else if t.code != TOKEN_PAREN_E {
                        self.command_error(ERR_SYNTAX_ERROR);
                        return;
                    }
                    if m == 0 {
                        let mut n3 = n2;
                        loop {
                            m += 1;
                            n3 >>= 1;
                            if n3 == 0 {
                                break;
                            }
                        }
                    }
                    for i in (0..m).rev() {
                        self.put_chr(b'0' + ((n2 >> i) & 1) as u8);
                    }
                }
                TOKEN_STR => {
                    let n = self.token_expression();
                    if self.err != 0 {
                        return;
                    }
                    let mut m: i16 = -1;
                    let t = self.token_get();
                    if t.code == TOKEN_COMMA {
                        m = self.token_expression();
                        if self.err != 0 {
                            return;
                        }
                        let t = self.token_get();
                        if t.code != TOKEN_PAREN_E {
                            self.command_error(ERR_SYNTAX_ERROR);
                            return;
                        }
                    } else if t.code != TOKEN_PAREN_E {
                        self.command_error(ERR_SYNTAX_ERROR);
                        return;
                    }
                    self.put_strmem(n as i32, m);
                }
                TOKEN_ERROR => {
                    self.command_error(ERR_SYNTAX_ERROR);
                    return;
                }
                _ => {
                    self.token_back();
                    let n = self.token_expression();
                    if self.err != 0 {
                        return;
                    }
                    self.put_num(n as i32);
                }
            }
            retflg = true;
            let t = self.token_get();
            if t.code == TOKEN_NULL || t.code == TOKEN_ELSE {
                self.token_back();
                break;
            }
            if t.code == TOKEN_COMMA {
                self.put_chr(b' ');
            } else if t.code == TOKEN_SEMICOLON {
                retflg = false;
            } else {
                self.command_error(ERR_SYNTAX_ERROR);
                return;
            }
        }
        if retflg {
            self.put_chr(b'\n');
        }
        self.token_end();
    }

    fn command_input(&mut self) {
        // MVP: INPUT は対話入力非対応のため値 0 を代入
        let t = self.token_get();
        let target: Option<usize> = if t.code == TOKEN_STRING {
            self.token_puts();
            let t = self.token_get();
            if t.code != TOKEN_COMMA {
                self.command_error(ERR_SYNTAX_ERROR);
                return;
            }
            let t = self.token_get();
            match t.code {
                TOKEN_VAR => Some(t.value as usize),
                TOKEN_ARRAY => {
                    let i = self.token_get_array_index();
                    if self.err != 0 {
                        return;
                    }
                    Some(i)
                }
                _ => {
                    self.command_error(ERR_SYNTAX_ERROR);
                    return;
                }
            }
        } else {
            self.put_chr(b'?');
            match t.code {
                TOKEN_VAR => Some(t.value as usize),
                TOKEN_ARRAY => {
                    let i = self.token_get_array_index();
                    if self.err != 0 {
                        return;
                    }
                    Some(i)
                }
                _ => {
                    self.command_error(ERR_SYNTAX_ERROR);
                    return;
                }
            }
        };
        self.token_end();
        if let Some(v) = target {
            self.var_set(v, 0);
        }
        self.put_chr(b'\n');
    }

    fn command_new(&mut self) {
        self.token_end();
        if self.err != 0 {
            return;
        }
        for b in &mut self.ram[OFFSET_RAM_LIST..OFFSET_RAM_LIST + SIZE_RAM_LIST] {
            *b = 0;
        }
        self.listsize = 0;
        self.pc = PC_NULL;
        self.pcbreak = PC_NULL;
    }

    fn command_list(&mut self) {
        let mut min = 0i16;
        let mut max = 0i32;
        if self.token_get_char() != 0 {
            min = self.token_expression();
            if self.err != 0 {
                return;
            }
            let code = self.token_get().code;
            match code {
                TOKEN_COMMA => {
                    max = self.token_expression() as i32;
                    if self.err != 0 {
                        return;
                    }
                }
                TOKEN_NULL | TOKEN_ELSE => {
                    if min < 0 {
                        max = -min as i32;
                        min = 0;
                    } else {
                        max = min as i32;
                    }
                    self.token_back();
                }
                _ => {
                    self.command_error(ERR_SYNTAX_ERROR);
                    return;
                }
            }
        }
        if max == 0 {
            max = 1 << 14;
            if min as i32 > max {
                max = -1;
            }
        }
        self.token_end();
        if self.err != 0 {
            return;
        }
        let mut index: u16 = 0;
        loop {
            let num = self.list_get_number(index);
            if num == 0 || num as i32 > max {
                break;
            }
            if num >= min {
                self.put_num(num as i32);
                self.put_chr(b' ');
                // 行内容
                let s_start = OFFSET_RAM_LIST + index as usize + 3;
                let mut p = s_start;
                while p < self.ram.len() && self.ram[p] != 0 {
                    let c = self.ram[p];
                    self.put_chr(c);
                    p += 1;
                }
                self.put_chr(b'\n');
            }
            index = index
                .wrapping_add(self.list_get_length(index) as u16)
                .wrapping_add(4);
        }
    }

    fn command_run(&mut self) {
        self.token_end();
        self.ngosubstack = 0;
        self.nforstack = 0;
        self.key_clear_key();
        if self.listsize > 0 {
            self.list_set_pc(0);
        } else {
            self.pc = PC_NULL;
            self.pcbreak = PC_NULL;
        }
    }

    fn command_end(&mut self) {
        self.token_end();
        self.pc = PC_NULL;
        self.pcbreak = PC_NULL;
    }

    fn command_led(&mut self) {
        let n = self.token_expression();
        if self.err != 0 {
            return;
        }
        self.led = n != 0;
        self.token_end();
    }

    fn command_out(&mut self) {
        let _ = self.token_expression();
        if self.err != 0 {
            return;
        }
        let code = self.token_get().code;
        if code != TOKEN_COMMA {
            self.token_back();
        } else {
            let _ = self.token_expression();
        }
        self.token_end();
    }

    fn command_clo(&mut self) {
        self.token_end();
    }

    fn command_wait(&mut self) {
        let n = self.token_expression();
        if self.err != 0 {
            return;
        }
        let _ = self.token_option1(1);
        // フレームベースの協調的待機。UI アプリは毎フレーム wait_frames を
        // 1 ずつ減らし、0 になるまで basic_step を呼ばない。
        self.wait_frames = self
            .wait_frames
            .saturating_add(n.max(0) as u32);
    }

    fn command_cls(&mut self) {
        self.token_end();
        self.screen_clear();
    }

    fn command_clt(&mut self) {
        self.token_end();
        self.video_clt();
    }

    fn command_clv(&mut self) {
        self.token_end();
        self.clear_vars();
    }

    fn command_locate(&mut self) {
        let x = self.token_expression();
        if self.err != 0 {
            return;
        }
        let code = self.token_get().code;
        let (x, y) = if code == TOKEN_COMMA {
            let y = self.token_expression();
            let code = self.token_get().code;
            if code == TOKEN_COMMA {
                self.cursorflg = self.token_expression() != 0;
            } else {
                self.cursorflg = false;
                self.token_back();
            }
            (x as i32, y as i32)
        } else {
            self.token_back();
            let y = calc_div(x as i32, self.screenw as i32);
            let x = calc_mod(x as i32, self.screenw as i32);
            (x, y)
        };
        if self.err != 0 {
            return;
        }
        self.token_end();
        self.screen_locate(x, y);
    }

    fn command_scroll(&mut self) {
        let dir = self.token_expression();
        if self.err != 0 {
            return;
        }
        self.token_end();
        self.screen_scroll(dir as i32);
    }

    fn command_poke(&mut self) {
        let mut n1 = self.token_expression();
        if self.err != 0 {
            return;
        }
        let code = self.token_get().code;
        if code != TOKEN_COMMA {
            self.command_error(ERR_SYNTAX_ERROR);
            return;
        }
        let n2 = self.token_expression();
        if self.err != 0 {
            return;
        }
        self.poke(n1 as i32, n2 as u8);
        loop {
            let code = self.token_get().code;
            if code != TOKEN_COMMA {
                self.token_back();
                self.token_end();
                return;
            }
            n1 = n1.wrapping_add(1);
            let n2 = self.token_expression();
            if self.err != 0 {
                return;
            }
            self.poke(n1 as i32, n2 as u8);
        }
    }

    fn command_copy(&mut self) {
        let mut dst = self.token_expression();
        if self.err != 0 {
            return;
        }
        let code = self.token_get().code;
        if code != TOKEN_COMMA {
            self.command_error(ERR_SYNTAX_ERROR);
            return;
        }
        let mut src = self.token_expression();
        if self.err != 0 {
            return;
        }
        let code = self.token_get().code;
        if code != TOKEN_COMMA {
            self.command_error(ERR_SYNTAX_ERROR);
            return;
        }
        let len = self.token_expression();
        if self.err != 0 {
            return;
        }
        if len > 0 {
            for _ in 0..len {
                let v = self.peek(src as i32);
                self.poke(dst as i32, v);
                dst = dst.wrapping_add(1);
                src = src.wrapping_add(1);
            }
        } else {
            for _ in 0..(-len) {
                let v = self.peek(src as i32);
                self.poke(dst as i32, v);
                dst = dst.wrapping_sub(1);
                src = src.wrapping_sub(1);
            }
        }
        self.token_end();
    }

    fn command_clp(&mut self) {
        self.screen_clp();
        self.token_end();
    }

    fn command_clk(&mut self) {
        self.key_clear_key();
        self.token_end();
    }

    // ============================================================
    // SAVE / LOAD / LRUN / FILES (ホストストレージ経由)
    // ============================================================

    fn command_save(&mut self) {
        let mut n = self.lastfile as i16;
        let code = self.token_get().code;
        self.token_back();
        if code != TOKEN_NULL && code != TOKEN_ELSE {
            n = self.token_expression();
            if self.err != 0 {
                return;
            }
        }
        self.token_end();
        if self.err != 0 {
            return;
        }

        let listsize = self.listsize as usize;
        let data: Vec<u8> = self.ram[OFFSET_RAM_LIST..OFFSET_RAM_LIST + listsize].to_vec();
        let ok = if let Some(s) = self.storage.as_mut() {
            s.save(n as u8, &data)
        } else {
            false
        };
        if ok {
            self.lastfile = n as u8;
            if !self.noresmode {
                self.put_str("Saved ");
                self.put_num(listsize as i32);
                self.put_str("byte\n");
            }
        } else {
            self.command_error(ERR_FILE_ERROR);
        }
    }

    fn command_load(&mut self, lrun: bool) {
        let mut n = self.lastfile as i16;
        let code = self.token_get().code;
        self.token_back();
        if code != TOKEN_NULL && code != TOKEN_ELSE {
            n = self.token_expression();
            if self.err != 0 {
                return;
            }
        }
        let mut m: i16 = 0;
        if lrun {
            let code = self.token_get().code;
            if code == TOKEN_COMMA {
                m = self.token_expression();
                if self.err != 0 {
                    return;
                }
            } else {
                self.token_back();
            }
        }
        self.token_end();
        if self.err != 0 {
            return;
        }

        // LIST 領域クリア
        for b in &mut self.ram[OFFSET_RAM_LIST..OFFSET_RAM_LIST + SIZE_RAM_LIST] {
            *b = 0;
        }
        self.listsize = 0;
        self.pc = PC_NULL;
        self.pcbreak = PC_NULL;

        // 読み込み
        let max = SIZE_RAM_LIST - 2;
        let mut buf = vec![0u8; max];
        let read = if let Some(s) = self.storage.as_mut() {
            s.load(n as u8, &mut buf)
        } else {
            -1
        };
        if read < 0 {
            self.command_error(ERR_FILE_ERROR);
            return;
        }
        let read = read as usize;
        self.ram[OFFSET_RAM_LIST..OFFSET_RAM_LIST + read].copy_from_slice(&buf[..read]);

        // listsize を行を辿って算出
        let mut index: u16 = 0;
        let mut bk_num = 0i16;
        loop {
            let num = self.list_get_number(index);
            if num == 0 {
                break;
            }
            if num <= bk_num {
                self.command_error(ERR_FILE_ERROR);
                return;
            }
            bk_num = num;
            let next = (index as usize)
                + self.list_get_length(index) as usize
                + 4;
            if next >= max {
                self.command_error(ERR_FILE_ERROR);
                return;
            }
            index = next as u16;
        }
        self.listsize = index;
        self.lastfile = n as u8;

        if !lrun && !self.noresmode {
            self.put_str("Loaded ");
            self.put_num(index as i32);
            self.put_str("byte\n");
        }

        if lrun {
            self.ngosubstack = 0;
            self.nforstack = 0;
            if self.listsize > 0 {
                let start = if m > 0 {
                    let i = self.list_find_goto(m);
                    if i < 0 {
                        self.command_error(ERR_UNDEFINED_LINE);
                        return;
                    }
                    i as u16
                } else {
                    0
                };
                self.list_set_pc(start);
            }
        }
    }

    fn command_files(&mut self) {
        let slot_count = self
            .storage
            .as_ref()
            .map(|s| s.slot_count())
            .unwrap_or(0);
        let mut endn = slot_count.saturating_sub(1) as i16;
        let mut startn = 0i16;
        if self.token_get_char() != 0 {
            endn = self.token_expression();
            if self.err != 0 {
                return;
            }
            let t = self.token_get();
            if t.code != TOKEN_COMMA {
                self.token_back();
            } else {
                startn = endn;
                endn = self.token_expression();
                if self.err != 0 {
                    return;
                }
            }
        }
        self.token_end();
        if self.err != 0 {
            return;
        }

        const PEEK_LEN: usize = SCREEN_W;
        let mut buf = [0u8; PEEK_LEN];
        for i in startn..=endn {
            if i < 0 {
                continue;
            }
            let res = if let Some(s) = self.storage.as_mut() {
                s.peek(i as u8, &mut buf)
            } else {
                -1
            };
            let b = self.put_num(i as i32);
            if res >= PEEK_LEN as i32 {
                let line_num = i16::from_le_bytes([buf[0], buf[1]]);
                if line_num > 0 {
                    self.put_chr(b' ');
                    let mut len = buf[2] as usize;
                    let max_show = PEEK_LEN.saturating_sub(3 + b as usize);
                    if len > max_show {
                        len = max_show;
                    }
                    for &c in &buf[3..3 + len] {
                        if c == 0 {
                            break;
                        }
                        self.put_chr(c);
                    }
                }
            }
            self.put_chr(b'\n');
        }
    }

    fn command_help(&mut self) {
        self.put_str("#000 CHAR\n#700 PCG\n#800 VAR\n#900 VRAM\n#C00 LIST\n");
        self.token_end();
    }

    fn command_srnd(&mut self) {
        let n = self.token_expression();
        if self.err != 0 {
            return;
        }
        self.token_end();
        self.random_seed(n as i32);
    }

    fn command_at(&mut self) {
        // ラベル行はコメントとして扱う
        while self.pc < self.ram.len() && self.ram[self.pc] != 0 && self.ram[self.pc] != b':' {
            self.pc += 1;
        }
        self.token_end();
    }

    fn command_ok(&mut self) {
        let mut n = 0;
        if self.token_get_char() != 0 {
            n = if self.token_expression() == 2 { 1 } else { 0 };
            if self.err != 0 {
                return;
            }
        }
        self.token_end();
        self.noresmode = n != 0;
    }

    fn command_renum(&mut self) {
        // 簡易版: 番号と行間隔指定はサポートするが、GOTO/GOSUB の参照書き換えは省略
        let mut start = 10i16;
        if self.token_get_char() != 0 {
            start = self.token_expression();
        }
        let step = self.token_option1(10);
        if start <= 0 || step <= 0 {
            self.command_error(ERR_ILLEGAL_ARGUMENT);
            return;
        }
        let mut index: u16 = 0;
        let mut current = start;
        loop {
            let num = self.list_get_number(index);
            if num == 0 {
                break;
            }
            self.list_set_number(index, current);
            current = current.wrapping_add(step);
            index = index
                .wrapping_add(self.list_get_length(index) as u16)
                .wrapping_add(4);
        }
    }

    fn command_beep(&mut self) {
        let mut tone = 10i16;
        let mut len = 3i16;
        let code = self.token_get().code;
        self.token_back();
        if code != TOKEN_NULL && code != TOKEN_ELSE {
            tone = self.token_expression();
            if self.err != 0 {
                return;
            }
            let code = self.token_get().code;
            if code != TOKEN_COMMA {
                self.token_back();
            } else {
                len = self.token_expression();
                if self.err != 0 {
                    return;
                }
            }
        }
        self.token_end();
        self.psg_beep(tone, len);
    }

    fn command_play(&mut self) {
        let mut mml: Option<i32> = None;
        let code = self.token_get().code;
        self.token_back();
        if code != TOKEN_NULL && code != TOKEN_ELSE {
            let n = self.token_expression();
            mml = Some(n as i32);
        }
        self.psg_play_mml(mml);
        self.token_end();
    }

    fn command_tempo(&mut self) {
        let tempo = self.token_expression();
        if self.err != 0 {
            return;
        }
        self.token_end();
        self.psg_tempo(tempo);
    }

    fn command_draw(&mut self) {
        let mut pos = [0i32; 5];
        let mut i = 0;
        while i < 5 {
            pos[i] = self.token_expression() as i32;
            if self.err != 0 {
                return;
            }
            let code = self.token_get().code;
            if code != TOKEN_COMMA {
                break;
            }
            i += 1;
        }
        // i は受け入れた数 - 1 のような状態。元 C と整合させる
        let i = i + 1;
        if i == 1 {
            self.command_error(ERR_SYNTAX_ERROR);
            return;
        }
        let i = if i & 1 == 1 {
            pos[i] = 1;
            i + 1
        } else {
            i
        };
        self.token_end();
        if i == 2 {
            self.screen_pset(pos[0], pos[1], pos[2]);
        } else {
            self.screen_line(pos[0], pos[1], pos[2], pos[3], pos[4]);
        }
    }

    fn token_option1(&mut self, default_value: i16) -> i16 {
        if self.err != 0 {
            return default_value;
        }
        let code = self.token_get().code;
        if code != TOKEN_COMMA {
            self.token_back();
            self.token_end();
            default_value
        } else {
            let v = self.token_expression();
            self.token_end();
            v
        }
    }
}

// ============================================================
// sin360: basic.h のテーブル参照そのまま
// ============================================================

const SIN_TABLE: [u8; 91] = [
    0, 3, 8, 12, 17, 21, 26, 30, 35, 39, 43, 48, 52, 57, 61, 65, 70, 74, 78, 82, 87, 91, 95, 99,
    103, 107, 111, 115, 119, 123, 127, 131, 135, 138, 142, 146, 149, 153, 157, 160, 164, 167, 170,
    174, 177, 180, 183, 186, 189, 192, 195, 198, 201, 203, 206, 209, 211, 214, 216, 218, 221, 223,
    225, 227, 229, 231, 233, 235, 236, 238, 240, 241, 242, 244, 245, 246, 247, 248, 249, 250, 251,
    252, 253, 253, 254, 254, 254, 255, 255, 255, 255,
];

pub fn sin360(mut deg: i32) -> i32 {
    let mut pm = 1;
    if deg < 0 {
        deg = -deg;
        pm = -pm;
    }
    while deg > 360 {
        deg -= 360;
    }
    if deg > 180 {
        deg -= 180;
        pm = -pm;
    }
    if deg > 90 {
        deg = 180 - deg;
    }
    if deg == 0 {
        return 0;
    }
    pm * (SIN_TABLE[deg as usize] as i32 + 1)
}
