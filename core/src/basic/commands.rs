//! `command_*` 群 — 各文 (PRINT, FOR, IF, SAVE, …) と `command_edit`、
//! および `print_dec` / `print_radix`。共通パース処理は `super::tokenizer`。

use crate::errors::*;
use crate::machine::{calc_div, calc_mod, strlen8, Machine, PC_NULL};
use crate::ram::*;
use crate::tokens::*;

impl Machine {
    /// 行番号付きの入力で LIST 領域へ追加/削除を行う。
    /// プログラム実行中 (pc が LIST 内) は呼ばれない前提。
    pub(super) fn command_edit(&mut self, number: i16) {
        if number <= 0 || self.pc_in_list() {
            self.command_error(ERR_SYNTAX_ERROR);
            return;
        }
        let found = self.list_find(number);
        if self.list_get_number(found) == number {
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

        // 末尾スペース除去 (1 個は残す)
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

    // ---- 制御フロー ----

    pub(super) fn command_rem(&mut self) {
        while self.pc < self.ram.len() && self.ram[self.pc] != 0 {
            self.pc += 1;
        }
    }

    pub(super) fn command_let(&mut self, terminator: u16) {
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

    pub(super) fn command_if(&mut self) {
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
            // 偽分岐: ELSE か行末まで読み飛ばす
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

    pub(super) fn command_for(&mut self) {
        if self.nforstack as usize >= IJB_SIZEOF_FOR_STACK {
            self.command_error(ERR_STACK_OVERFLOW);
            return;
        }
        self.forstack[self.nforstack as usize] = self.pc;
        self.nforstack += 1;

        let Some(v) = self.parse_lvalue_index() else { return };
        let t = self.token_get();
        if t.code != TOKEN_EQ && t.code != TOKEN_COMMA {
            self.command_error(ERR_SYNTAX_ERROR);
            return;
        }
        let ival = self.command_let2(v);
        if self.err != 0 {
            return;
        }
        if !self.expect_token(TOKEN_TO) {
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

    pub(super) fn command_next(&mut self) {
        if self.nforstack == 0 {
            self.command_error(ERR_NOT_MATCH);
            return;
        }
        self.token_end();
        let bkpc = self.pc;
        self.pc = self.forstack[self.nforstack as usize - 1];
        let Some(v) = self.parse_lvalue_index() else { return };
        let t = self.token_get();
        if t.code != TOKEN_EQ && t.code != TOKEN_COMMA {
            self.command_error(ERR_SYNTAX_ERROR);
            return;
        }
        let ival = self.token_expression();
        if self.err != 0 {
            return;
        }
        if !self.expect_token(TOKEN_TO) {
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

    pub(super) fn command_goto(&mut self) {
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

    pub(super) fn command_gosub(&mut self) {
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

    pub(super) fn command_return(&mut self) {
        if self.ngosubstack == 0 {
            self.command_error(ERR_NOT_MATCH);
            return;
        }
        self.token_end();
        self.ngosubstack -= 1;
        self.pc = self.gosubstack[self.ngosubstack as usize];
    }

    pub(super) fn command_cont(&mut self) {
        self.token_end();
        if !self.pc_in_list() {
            self.pc = self.pcbreak;
        }
        if self.pc_in_list() {
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

    pub(super) fn command_end(&mut self) {
        self.token_end();
        self.pc = PC_NULL;
        self.pcbreak = PC_NULL;
    }

    pub(super) fn command_run(&mut self) {
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

    pub(super) fn command_at(&mut self) {
        // ラベル行はコメントとして扱う
        while self.pc < self.ram.len() && self.ram[self.pc] != 0 && self.ram[self.pc] != b':' {
            self.pc += 1;
        }
        self.token_end();
    }

    pub(super) fn command_ok(&mut self) {
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

    // ---- 入出力 (画面, 変数, ピクセル, GPIO no-op) ----

    pub(super) fn command_print(&mut self) {
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
                    let Some((n2, m)) = self.parse_format_args(0) else { return };
                    self.print_dec(n2, m);
                }
                TOKEN_HEX => {
                    let Some((n2, m)) = self.parse_format_args(0) else { return };
                    self.print_radix(n2 as u16, m, 4);
                }
                TOKEN_BIN => {
                    let Some((n2, m)) = self.parse_format_args(0) else { return };
                    self.print_radix(n2 as u16, m, 1);
                }
                TOKEN_STR => {
                    let Some((n, m)) = self.parse_format_args(-1) else { return };
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

    pub(super) fn command_input(&mut self) {
        // MVP: INPUT は対話入力非対応のため値 0 を代入
        let t = self.token_get();
        let target: Option<usize> = if t.code == TOKEN_STRING {
            self.token_puts();
            if !self.expect_token(TOKEN_COMMA) {
                return;
            }
            self.parse_lvalue_index()
        } else {
            self.put_chr(b'?');
            self.token_back();
            self.parse_lvalue_index()
        };
        if self.err != 0 {
            return;
        }
        self.token_end();
        if let Some(v) = target {
            self.var_set(v, 0);
        }
        self.put_chr(b'\n');
    }

    pub(super) fn command_new(&mut self) {
        self.token_end();
        if self.err != 0 {
            return;
        }
        self.ram[OFFSET_RAM_LIST..OFFSET_RAM_LIST + SIZE_RAM_LIST].fill(0);
        self.listsize = 0;
        self.pc = PC_NULL;
        self.pcbreak = PC_NULL;
    }

    pub(super) fn command_list(&mut self) {
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

    pub(super) fn command_led(&mut self) {
        let n = self.token_expression();
        if self.err != 0 {
            return;
        }
        self.led = n != 0;
        self.token_end();
    }

    pub(super) fn command_out(&mut self) {
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

    pub(super) fn command_clo(&mut self) {
        self.token_end();
    }

    pub(super) fn command_wait(&mut self) {
        let n = self.token_expression();
        if self.err != 0 {
            return;
        }
        let _ = self.token_option1(1);
        // フレームベースの協調的待機。UI アプリは毎フレーム wait_frames を
        // 1 ずつ減らし、0 になるまで basic_step を呼ばない。
        self.wait_frames = self.wait_frames.saturating_add(n.max(0) as u32);
    }

    pub(super) fn command_cls(&mut self) {
        self.token_end();
        self.screen_clear();
    }

    pub(super) fn command_clt(&mut self) {
        self.token_end();
        self.video_clt();
    }

    pub(super) fn command_clv(&mut self) {
        self.token_end();
        self.clear_vars();
    }

    pub(super) fn command_locate(&mut self) {
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

    pub(super) fn command_scroll(&mut self) {
        let dir = self.token_expression();
        if self.err != 0 {
            return;
        }
        self.token_end();
        self.screen_scroll(dir as i32);
    }

    pub(super) fn command_poke(&mut self) {
        let mut n1 = self.token_expression();
        if self.err != 0 {
            return;
        }
        if !self.expect_token(TOKEN_COMMA) {
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

    pub(super) fn command_copy(&mut self) {
        let mut dst = self.token_expression();
        if self.err != 0 {
            return;
        }
        if !self.expect_token(TOKEN_COMMA) {
            return;
        }
        let mut src = self.token_expression();
        if self.err != 0 {
            return;
        }
        if !self.expect_token(TOKEN_COMMA) {
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

    pub(super) fn command_clp(&mut self) {
        self.screen_clp();
        self.token_end();
    }

    pub(super) fn command_clk(&mut self) {
        self.key_clear_key();
        self.token_end();
    }

    pub(super) fn command_srnd(&mut self) {
        let n = self.token_expression();
        if self.err != 0 {
            return;
        }
        self.token_end();
        self.random_seed(n as i32);
    }

    pub(super) fn command_draw(&mut self) {
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
        // i は受け入れた数 - 1。元 C と整合させる
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

    // ---- SAVE / LOAD / LRUN / FILES (ホストストレージ経由) ----

    pub(super) fn command_save(&mut self) {
        let Some(n) = self.parse_optional_expr(self.lastfile as i16) else { return };
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

    pub(super) fn command_load(&mut self, lrun: bool) {
        let Some(n) = self.parse_optional_expr(self.lastfile as i16) else { return };
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

        self.ram[OFFSET_RAM_LIST..OFFSET_RAM_LIST + SIZE_RAM_LIST].fill(0);
        self.listsize = 0;
        self.pc = PC_NULL;
        self.pcbreak = PC_NULL;

        let max = SIZE_RAM_LIST - 2;
        let mut buf = vec![0u8; max];
        let read = self
            .storage
            .as_mut()
            .and_then(|s| s.load(n as u8, &mut buf));
        let Some(read) = read else {
            self.command_error(ERR_FILE_ERROR);
            return;
        };
        self.ram[OFFSET_RAM_LIST..OFFSET_RAM_LIST + read].copy_from_slice(&buf[..read]);

        // 行を辿って listsize を再算出 (壊れた SAVE データの検出を兼ねる)
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
            let next = (index as usize) + self.list_get_length(index) as usize + 4;
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

    pub(super) fn command_files(&mut self) {
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
            let res = self
                .storage
                .as_mut()
                .and_then(|s| s.peek(i as u8, &mut buf));
            let b = self.put_num(i as i32);
            if res.is_some_and(|n| n >= PEEK_LEN) {
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

    pub(super) fn command_help(&mut self) {
        self.put_str("#000 CHAR\n#700 PCG\n#800 VAR\n#900 VRAM\n#C00 LIST\n");
        self.token_end();
    }

    pub(super) fn command_renum(&mut self) {
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

    // ---- PSG (BEEP / PLAY / TEMPO) ----

    pub(super) fn command_beep(&mut self) {
        // 既定値は IchigoJam 標準の TONE=10, LEN=3。
        let code = self.token_get().code;
        self.token_back();
        let (tone, len) = if code == TOKEN_NULL || code == TOKEN_ELSE {
            (10i16, 3i16)
        } else {
            let tone = self.token_expression();
            if self.err != 0 {
                return;
            }
            let len = if self.token_get().code == TOKEN_COMMA {
                let v = self.token_expression();
                if self.err != 0 {
                    return;
                }
                v
            } else {
                self.token_back();
                3
            };
            (tone, len)
        };
        self.token_end();
        self.psg_beep(tone, len);
    }

    pub(super) fn command_play(&mut self) {
        let code = self.token_get().code;
        self.token_back();
        let mml = if code == TOKEN_NULL || code == TOKEN_ELSE {
            None
        } else {
            let n = self.token_expression();
            if self.err != 0 {
                return;
            }
            Some(n as i32)
        };
        self.psg_play_mml(mml);
        self.token_end();
    }

    pub(super) fn command_tempo(&mut self) {
        let tempo = self.token_expression();
        if self.err != 0 {
            return;
        }
        self.token_end();
        self.psg_tempo(tempo);
    }

    // ---- PRINT のフォーマット出力ヘルパ ----

    /// `m <= 0` で無装飾、`m > 0` で右寄せ。桁あふれ時は下位 m 桁を符号無しで。
    fn print_dec(&mut self, n2: i16, m: i16) {
        if m <= 0 {
            self.put_num(n2 as i32);
            return;
        }
        let beam = Machine::beam(n2 as i32);
        if (beam as i16) <= m {
            for _ in 0..(m as u32 - beam) {
                self.put_chr(b' ');
            }
            self.put_num(n2 as i32);
            return;
        }
        // 桁数オーバー: 下位 m 桁のみ出力 (符号は捨てる)
        let mut n2 = (n2 as i32).abs();
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

    /// `bits_per_digit` は HEX=4 / BIN=1。`m == 0` は最小桁数で出力。
    fn print_radix(&mut self, value: u16, mut m: i16, bits_per_digit: u32) {
        if m == 0 {
            let mut n = value;
            loop {
                m += 1;
                n >>= bits_per_digit;
                if n == 0 {
                    break;
                }
            }
        }
        for i in (0..m).rev() {
            let shift = i as u32 * bits_per_digit;
            let mask = (1u16 << bits_per_digit) - 1;
            let digit = ((value >> shift) & mask) as u8;
            let c = if digit >= 10 {
                digit + b'A' - 10
            } else {
                digit + b'0'
            };
            self.put_chr(c);
        }
    }
}
