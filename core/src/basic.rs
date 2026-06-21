//! BASIC インタプリタの外殻 (実行ループとディスパッチ)。
//!
//! 各文/式の実装は [`commands`] / [`expr`] / [`tokenizer`] / [`sin`] に分かれる。

mod commands;
mod expr;
mod sin;
mod tokenizer;

pub use sin::sin360;

use crate::errors::*;
use crate::machine::{BasicResult, Machine, PC_NULL};
use crate::ram::*;
use crate::tokens::*;

// TOKEN_VER 関数が返す仕様バージョン定数。
pub(crate) const IJB_VER: i32 = 143;
pub(crate) const IJB_BUILD: i32 = 28;
pub(crate) const LANG_JP: i32 = 1;
pub(crate) const VER_PLATFORM_PC: i32 = 4;

impl Machine {
    /// `commandline_pc` は RAM インデックス。
    pub fn basic_start(&mut self, commandline_pc: usize) {
        self.last_error = None;
        self.ngosubstack = 0;
        self.nforstack = 0;
        self.tokenmode = 0;
        self.pc = commandline_pc;
        self.lasttoken = 0;
        self.lasttokenpc = 0;
    }

    /// 1 文ぶんだけ実行する。返り値が `Some` なら停止 (理由付き)、`None`
    /// なら継続。協調的実行のため egui アプリは毎フレーム複数回呼ぶ。
    ///
    /// 実体は [`Machine::step_once`]。エラー (または ESC ブレーク) は `Err`
    /// として伝搬され、ここで 1 度だけ表示・記録して `StopOrErr` に変換する。
    pub fn basic_step(&mut self) -> Option<BasicResult> {
        match self.step_once() {
            Ok(result) => result,
            Err(e) => {
                self.last_error = Some(e);
                self.basic_print_error(e);
                Some(BasicResult::StopOrErr)
            }
        }
    }

    /// 1 文を実行する内部本体。`Ok(Some(_))` で停止、`Ok(None)` で継続、
    /// `Err(_)` でエラー/中断。
    fn step_once(&mut self) -> BResult<Option<BasicResult>> {
        if self.pc == PC_NULL {
            return Ok(Some(BasicResult::Execute));
        }
        self.token_get_char();
        if self.pc == PC_NULL {
            return Ok(Some(BasicResult::Execute));
        }
        let c = self.ram_at(self.pc);
        if c == b':' {
            self.pc += 1;
            return Ok(None);
        }
        if c == b'\'' {
            self.command_rem();
            return Ok(None);
        }
        if c == 0 {
            return Ok(self.handle_statement_terminator());
        }

        let token = self.token_get();

        // 行番号付き入力 = 行編集モード (LIST 領域への追加・削除)
        if token.code == TOKEN_NUMBER {
            self.command_edit(token.value)?;
            self.pc = PC_NULL;
            return Ok(Some(BasicResult::Edit));
        }

        self.dispatch_command(token.code)?;

        if self.stop_execute() {
            return Err(ERR_BREAK);
        }
        Ok(None)
    }

    /// 1 行ぶんを実行する。RUN/GOTO 等で PC が LIST 領域へ移動した場合は
    /// 呼出元 (UI アプリ) へ制御を返し、以降は `basic_step` を毎フレーム
    /// 呼び出してプログラムを進める。
    pub fn basic_execute(&mut self, commandline_pc: usize) -> BasicResult {
        // プログラム実行中の画面出力 (PRINT 等) は上書きモードに固定する。
        // 対話編集の挿入/上書きはホストが各キー処理前に sync_insert_mode で復元する。
        self.screen_insertmode = true;
        // 実行中はカーソルを非表示にする。REPL に戻るとホストが cursorflg を
        // 再び立てる。プログラム側の LOCATE x,y,1 による明示表示は引き続き可能。
        self.cursorflg = false;
        self.basic_start(commandline_pc);
        let started_in_list =
            (OFFSET_RAM_LIST..OFFSET_RAM_LIST + SIZE_RAM_LIST).contains(&commandline_pc);
        loop {
            if let Some(r) = self.basic_step() {
                return r;
            }
            if self.wait_frames > 0 {
                return BasicResult::Execute;
            }
            // 即時入力 → プログラム実行への遷移を検知
            if !started_in_list && self.pc_in_list() {
                return BasicResult::Execute;
            }
        }
    }

    pub(crate) fn ram_at(&self, pc: usize) -> u8 {
        self.ram.get(pc).copied().unwrap_or(0)
    }

    fn handle_statement_terminator(&mut self) -> Option<BasicResult> {
        // LIST 領域では、ステートメントは偶数バイトに揃えられ、奇数位置に
        // 終端 NULL がある。偶数 PC で NULL に当たった場合 (= パディング NULL)
        // は +1 して実際の終端へ進める (C 版 AddrIsOdd 相当)。
        if self.pc_in_list() && (self.pc & 1) == (OFFSET_RAM_LIST & 1) {
            self.pc += 1;
        }
        if self.pc >= OFFSET_RAM_LIST
            && self.pc + 4 < OFFSET_RAM_LIST + self.listsize as usize
        {
            self.pc += 4;
            return None;
        }
        Some(BasicResult::Execute)
    }

    /// `TOKEN_NUMBER` (行編集) は basic_step 側で別途処理される。
    /// 未知トークンは `Err(Syntax error)` を返す。
    fn dispatch_command(&mut self, code: u16) -> BResult<()> {
        match code {
            TOKEN_NULL => {}
            TOKEN_VAR | TOKEN_ARRAY => {
                self.token_back();
                self.command_let(TOKEN_EQ)?;
            }
            TOKEN_AT => self.command_at()?,
            TOKEN_IF => self.command_if()?,
            TOKEN_ELSE => self.command_rem(),
            TOKEN_FOR => self.command_for()?,
            TOKEN_NEXT => self.command_next()?,
            TOKEN_GOTO => self.command_goto()?,
            TOKEN_GOSUB_1 | TOKEN_GOSUB_2 => self.command_gosub()?,
            TOKEN_RETURN_1 | TOKEN_RETURN_2 => self.command_return()?,
            TOKEN_END | TOKEN_STOP => self.command_end()?,
            TOKEN_REM_1 | TOKEN_REM_2 => self.command_rem(),
            TOKEN_CONT => self.command_cont()?,
            TOKEN_OK => self.command_ok()?,
            TOKEN_NEW => self.command_new()?,
            TOKEN_RUN => self.command_run()?,
            TOKEN_LET => self.command_let(TOKEN_COMMA)?,
            TOKEN_CLS => self.command_cls()?,
            TOKEN_LOCATE_1 | TOKEN_LOCATE_2 => self.command_locate()?,
            TOKEN_PRINT_1 | TOKEN_PRINT_2 => self.command_print()?,
            TOKEN_INPUT => self.command_input()?,
            TOKEN_CLV_1 | TOKEN_CLV_2 => self.command_clv()?,
            TOKEN_CLK => self.command_clk()?,
            TOKEN_KBD => self.command_kbd()?,
            TOKEN_SRND => self.command_srnd()?,
            TOKEN_DRAW => self.command_draw()?,
            TOKEN_WAIT => self.command_wait()?,
            TOKEN_CLT => self.command_clt()?,
            TOKEN_OUT => self.command_out()?,
            TOKEN_LED => self.command_led()?,
            TOKEN_CLO => self.command_clo()?,
            TOKEN_RENUM => self.command_renum()?,
            TOKEN_SCROLL => self.command_scroll()?,
            TOKEN_VIDEO => self.command_video()?,
            TOKEN_BEEP => self.command_beep()?,
            TOKEN_TEMPO => self.command_tempo()?,
            TOKEN_PLAY => self.command_play()?,
            TOKEN_POKE => self.command_poke()?,
            TOKEN_COPY => self.command_copy()?,
            TOKEN_CLP => self.command_clp()?,
            TOKEN_LIST => self.command_list()?,
            TOKEN_LOAD => self.command_load(false)?,
            TOKEN_LRUN => self.command_load(true)?,
            TOKEN_SAVE => self.command_save()?,
            TOKEN_FILES => self.command_files()?,
            TOKEN_HELP => self.command_help()?,
            _ => return Err(ERR_SYNTAX_ERROR),
        }
        Ok(())
    }
}
