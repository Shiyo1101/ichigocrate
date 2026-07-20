//! ホスト共通のセッション駆動層。
//!
//! REPL・プログラム実行・INPUT 対話・WAIT・60Hz tick からなる実行状態機械を
//! ここに集約する。ネイティブ (egui) / Web (wasm) のフロントエンドは
//! 「キーイベントの変換」「描画」「音声」「ストレージ実装」だけを持ち、
//! 状態遷移はすべて [`Session`] のメソッドを通す。
//!
//! かつて両フロントエンドが同じ状態機械を別々に実装しており、修正が二重に
//! なるうえ挙動も乖離していた (F1 の CLS 抑止バグはその典型)。"OK" の表示
//! 判定を含む遷移を一箇所へ寄せるのが本モジュールの目的。
//!
//! 時刻はミリ秒 (`f64`) で外部から注入する。ネイティブは起動からの経過 ms、
//! Web は `performance.now()` をそのまま渡せばよい。

use crate::errors::BasicError;
use crate::keycodes as kc;
use crate::machine::{BasicResult, Machine, PC_NULL};
use crate::ram::{OFFSET_RAM_VRAM, SIZE_RAM_VRAM};
use crate::{exec_line_bytes, LineOutcome};

/// IchigoJam の論理 1 フレーム = 1/60 秒 (ミリ秒)。
pub const FRAME_MS: f64 = 1000.0 / 60.0;

/// 1 フレームで進める最大文数。無限ループのプログラムでも UI を凍結させない。
const MAX_STEPS_PER_FRAME: usize = 2000;

/// IchigoJam 標準準拠の F1-F9 コマンド割当。bool は「Enter まで自動実行するか」。
/// false のものはカーソルを残し、ユーザがスロット番号などを続けて入力できる。
const FKEY_BINDINGS: [(&str, bool); 9] = [
    ("CLS", true),
    ("LOAD", false),
    ("SAVE", false),
    ("LIST", true),
    ("RUN", true),
    ("?FREE()", true),
    ("?VER()", true),
    ("VIDEO", false),
    ("FILES", true),
];

/// F キー番号 (F1=1 .. F9=9) の割当を返す。
pub fn fkey_binding(n: u8) -> Option<(&'static str, bool)> {
    FKEY_BINDINGS.get(n.wrapping_sub(1) as usize).copied()
}

/// keymap の戻り値のうち REPL 編集を進める「制御コード」群。
/// これらは `input_control` 経由で画面エディタへ流す。
pub fn is_edit_control_code(c: u8) -> bool {
    matches!(
        c,
        kc::BACKSPACE
            | kc::DELETE
            | kc::CURSOR_LEFT
            | kc::CURSOR_RIGHT
            | kc::CURSOR_UP
            | kc::CURSOR_DOWN
            | kc::TAB
            | kc::HOME
            | kc::END
            | kc::PAGE_UP
            | kc::PAGE_DOWN
            | kc::INSERT_TOGGLE
            | kc::LINE_SPLIT
    )
}

/// IchigoJam VM 1 台ぶんの実行セッション。
///
/// `machine` は描画 (VRAM/PCG)・キー押下状態 (BTN)・keymap 参照などのために
/// フロントエンドから直接読み書きしてよいが、実行状態 (プログラム開始/停止・
/// INPUT 対話・"OK" 表示) を動かす操作は必ず Session のメソッドを使うこと。
pub struct Session {
    pub machine: Machine,
    /// プログラム実行中フラグ (REPL 行確定や RUN で true)。
    is_running: bool,
    /// INPUT 対話入力待ち中の値開始 VRAM 座標 (cursorx, cursory)。
    input_origin: Option<(i32, i32)>,
    /// 次回の実行完了時に "OK" 表示を抑止するフラグ。F1 (CLS) は画面を消す
    /// のが目的なので、直後に "OK" が出ると空白画面にならず UX を損なう。
    suppress_next_ok: bool,
    /// 60Hz tick を次に進める基準時刻 (ms)。`None` は初回 tick 未到達。
    next_tick_ms: Option<f64>,
    /// WAIT の実時間終了予定時刻 (ms)。
    wait_until_ms: Option<f64>,
}

impl Session {
    /// ストレージ設定済みの `machine` を受け取り、電源 ON 状態で開始する。
    pub fn new(mut machine: Machine) -> Self {
        machine.power_on_reset();
        machine.put_str("OK\n");
        Self {
            machine,
            is_running: false,
            input_origin: None,
            suppress_next_ok: false,
            next_tick_ms: None,
            wait_until_ms: None,
        }
    }

    /// プログラム実行中か (REPL 待機の対義)。
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// INPUT 文の対話入力待ち中か。
    pub fn is_awaiting_input(&self) -> bool {
        self.input_origin.is_some()
    }

    /// WAIT の実時間待ち中か。フロントエンドの再描画スケジュールの判断に使う。
    pub fn is_waiting(&self) -> bool {
        self.wait_until_ms.is_some()
    }

    /// 実機の RESET ボタン (電源 ON/OFF による再起動) 相当。
    pub fn reset(&mut self) {
        self.machine.power_on_reset();
        self.machine.put_str("OK\n");
        self.is_running = false;
        self.input_origin = None;
        self.suppress_next_ok = false;
        self.wait_until_ms = None;
    }

    /// 実行中プログラムの中断要求 (ESC 相当)。次の step で Break になる。
    pub fn break_program(&mut self) {
        self.machine.is_esc_pressed = true;
    }

    /// 1 フレーム進める。60Hz tick (PSG/frames) の実時間同期、WAIT 期限の管理、
    /// 実行中プログラムの継続実行を行う。プログラムがエラーで停止したときは
    /// その理由を返す (画面へのエラーメッセージは出力済み)。
    pub fn tick(&mut self, now_ms: f64) -> Option<BasicError> {
        let next = self.next_tick_ms.get_or_insert(now_ms);
        // 60Hz tick を実時間に同期して必要回数進める。大きく遅れたら追いつく
        // のを諦めて基準をリセット (バックグラウンド放置後のバースト防止)。
        let mut iters = 0;
        while *next <= now_ms && iters < 8 {
            self.machine.frames = self.machine.frames.wrapping_add(1);
            self.machine.psg_tick();
            *next += FRAME_MS;
            iters += 1;
        }
        if *next + FRAME_MS * 8.0 < now_ms {
            *next = now_ms;
        }

        if let Some(deadline) = self.wait_until_ms {
            if now_ms >= deadline {
                self.wait_until_ms = None;
            }
        }
        // BASIC 側で積まれた WAIT を実時間の期限へ変換する。
        if self.machine.wait_frames > 0 {
            let extra = FRAME_MS * f64::from(self.machine.wait_frames);
            let base = self.wait_until_ms.unwrap_or(now_ms);
            self.wait_until_ms = Some(base + extra);
            self.machine.wait_frames = 0;
        }

        self.machine.is_program_running = self.is_running;

        let err = if self.is_running && self.wait_until_ms.is_none() {
            self.step_chunk()
        } else {
            None
        };

        // 待機 (REPL) 中はカーソルを表示し挿入モードを同期する。毎フレーム
        // 行わないと、コマンド実行後に次のキー入力まで点滅カーソルが出ない。
        self.sync_before_input();
        err
    }

    /// Enter キー 1 押下ぶんの処理。実行中は keybuf (INKEY/INPUT) へ、INPUT
    /// 待ち中は値確定、REPL 中は現在行の実行。即時実行がエラーで止まった
    /// ときはその理由を返す。
    pub fn on_enter(&mut self) -> Option<BasicError> {
        self.sync_before_input();
        if self.machine.is_executing() {
            self.machine.key_push(b'\n');
            return None;
        }
        if self.input_origin.is_some() {
            self.complete_input();
            return None;
        }
        self.execute_current_line()
    }

    /// ESC キー 1 押下ぶんの処理。実行中なら Break 要求、INPUT 待ち中なら
    /// 入力を破棄して REPL へ戻る。
    pub fn on_escape(&mut self) {
        self.machine.is_esc_pressed = true;
        if self.input_origin.is_some() {
            self.cancel_input();
        }
    }

    /// 解決済みの 1 文字をモード適応で流す。実行中は keybuf (INKEY/INPUT) へ、
    /// 停止中は REPL 行編集へ振り分ける。改行は [`Self::on_enter`] と同じ扱い。
    pub fn feed_char(&mut self, c: u8) -> Option<BasicError> {
        self.sync_before_input();
        if self.machine.is_executing() {
            self.machine.key_push(c);
            return None;
        }
        match c {
            b'\n' | b'\r' => return self.on_enter(),
            // カナモード中の Backspace は未確定バッファ管理のため input_putc を通す。
            _ if is_edit_control_code(c) => {
                if c == kc::BACKSPACE && self.machine.is_kana_mode {
                    self.machine.input_putc(c);
                } else {
                    self.machine.input_control(c);
                }
            }
            // グラフィック文字 (128-255) はローマ字 → カナ変換を通さない。
            _ if c >= 128 => self.machine.screen_putc(c),
            _ => self.machine.input_putc(c),
        }
        None
    }

    /// F キー (F1=1 .. F9=9) の割当コマンド投入。REPL 待機中のみ受理し、
    /// 実行中・INPUT 待ち中は何もしない。
    ///
    /// 本家同様、カーソル行に何が書かれていても消してから書き込むので、
    /// 編集途中の行があってもコマンドは常に単独で表示・実行される。
    pub fn press_fkey(&mut self, n: u8) -> Option<BasicError> {
        if self.is_running || self.input_origin.is_some() {
            return None;
        }
        let (cmd, run) = fkey_binding(n)?;
        self.sync_before_input();
        self.machine.screen_clear_line();
        for b in cmd.bytes() {
            self.machine.screen_putc(b);
        }
        if run {
            // CLS 直後は画面を空白のまま保ちたいので "OK" 表示を抑止する。
            self.suppress_next_ok = cmd == "CLS";
            return self.execute_current_line();
        }
        None
    }

    /// REPL 1 行を画面エディタを介さず直接実行する。停止中 (REPL) のみ受理し、
    /// 実行中・INPUT 待ち中は無視する。
    pub fn exec_line(&mut self, line: &[u8]) -> Option<BasicError> {
        if self.is_running || self.input_origin.is_some() {
            return None;
        }
        self.machine.is_program_running = false;
        self.machine.is_esc_pressed = false;
        match exec_line_bytes(&mut self.machine, line) {
            Ok(LineOutcome::Executed) => self.finish_executed(),
            // 行編集 (LIST 追加・削除) は OK を表示しない (IchigoJam 慣習)。
            Ok(LineOutcome::Edited) => None,
            Ok(LineOutcome::AwaitingInput) => {
                self.begin_input();
                None
            }
            // エラーメッセージは VRAM に出力済み (basic_print_error)。
            Err(e) => Some(e),
        }
    }

    /// Enter 確定時: カーソル行を生バイト列として取り出し REPL 実行する。
    fn execute_current_line(&mut self) -> Option<BasicError> {
        // Enter の改行を VRAM へ書き込んでから行を読む。
        self.machine.screen_putc(b'\n');
        let p = self.machine.screen_line_start();
        // VRAM から行長を測り生バイトのスライスを得る (String 経由は 0x80-0xFF
        // のグラフィック文字を UTF-8 に展開してしまうため不可)。
        let vram_end = OFFSET_RAM_VRAM + SIZE_RAM_VRAM;
        let len = self.machine.ram[p..vram_end]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(vram_end - p);
        if len == 0 {
            return None;
        }
        self.machine.is_esc_pressed = false;
        // Machine 借用のため一旦スライスをローカルにコピー。
        let line: Vec<u8> = self.machine.ram[p..p + len].to_vec();
        match exec_line_bytes(&mut self.machine, &line) {
            Ok(LineOutcome::Executed) => self.finish_executed(),
            Ok(LineOutcome::Edited) => None,
            Ok(LineOutcome::AwaitingInput) => {
                self.begin_input();
                None
            }
            Err(e) => Some(e),
        }
    }

    /// 行実行が `Executed` を返した後の共通処理。
    ///
    /// IchigoJam は実行後も pc を非 NULL に残し後続の step で完了する設計
    /// なので、即時文はここで 1 フレーム分まで同期実行して完了させる。
    /// 終わらなければ (RUN の無限ループ等) is_running を立ててフレーム側へ
    /// 委譲し、UI を固めない。
    fn finish_executed(&mut self) -> Option<BasicError> {
        if self.machine.pc == PC_NULL {
            if !std::mem::take(&mut self.suppress_next_ok) {
                self.machine.put_str("OK\n");
            }
            return None;
        }
        self.is_running = true;
        self.machine.is_program_running = true;
        if self.wait_until_ms.is_none() {
            return self.step_chunk();
        }
        None
    }

    /// プログラム実行を 1 フレーム分 (最大 [`MAX_STEPS_PER_FRAME`] 文) 進める。
    /// 完了・INPUT 待ち・WAIT 発火で打ち切る。ここが実行完了の唯一の到達点で、
    /// "OK" 表示と `suppress_next_ok` の消費もここで行う。
    fn step_chunk(&mut self) -> Option<BasicError> {
        for _ in 0..MAX_STEPS_PER_FRAME {
            if self.machine.wait_frames > 0 {
                break; // ステップ中に WAIT 発火 → 次フレームへ
            }
            if let Some(res) = self.machine.basic_step() {
                self.is_running = false;
                self.machine.is_esc_pressed = false;
                return match res {
                    BasicResult::Execute => {
                        if !std::mem::take(&mut self.suppress_next_ok) {
                            self.machine.put_str("OK\n");
                        }
                        None
                    }
                    BasicResult::Input => {
                        self.begin_input();
                        None
                    }
                    // エラーメッセージは VRAM に出力済み。停止理由を返す。
                    BasicResult::StopOrErr => self.machine.last_error(),
                    BasicResult::Edit => None,
                };
            }
            if self.machine.pc == PC_NULL {
                self.is_running = false;
                if !std::mem::take(&mut self.suppress_next_ok) {
                    self.machine.put_str("OK\n");
                }
                break;
            }
        }
        None
    }

    /// INPUT 入力待ちの開始。プロンプトは表示済みなので、現在のカーソル位置
    /// (プロンプト直後) を入力値の開始位置として記録する。
    fn begin_input(&mut self) {
        self.input_origin = Some((self.machine.cursorx, self.machine.cursory));
        self.machine.is_esc_pressed = false;
    }

    /// INPUT の入力確定。プロンプト直後から行末までの VRAM を値テキストとして
    /// 読み取り、変数へ反映して実行を再開する。
    fn complete_input(&mut self) {
        let (ox, oy) = self.input_origin.take().unwrap_or((0, 0));
        let w = self.machine.screen_cols();
        let start = OFFSET_RAM_VRAM + ox.max(0) as usize + oy.max(0) as usize * w;
        let vram_end = OFFSET_RAM_VRAM + SIZE_RAM_VRAM;
        let len = self.machine.ram[start..vram_end]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(vram_end - start);
        let line: Vec<u8> = self.machine.ram[start..start + len].to_vec();
        self.machine.input_complete(&line);
        // pc は INPUT 文の直後を指すので、実行を再開する。
        self.is_running = true;
    }

    /// INPUT 入力中の ESC 中断。代入せずに入力待ちを解除し REPL へ戻る。
    fn cancel_input(&mut self) {
        self.input_origin = None;
        self.machine.cancel_input();
        self.machine.put_str("OK\n");
        self.machine.is_esc_pressed = false;
    }

    /// キー入力処理の前にマシン状態をフレームの実行状況へ同期する。
    ///
    /// `is_program_running` を立てることで `input_putc`/`input_control` が
    /// 実行中の対話編集を無視する。判定に `pc` を使えないのが要点で、`pc` は
    /// STOP/ESC ブレーク後も CONT 用に残るため、停止しても入力が復活しなく
    /// なってしまう。非実行 (REPL) 中は挿入/上書きモードを同期しカーソルを
    /// 表示する。
    fn sync_before_input(&mut self) {
        self.machine.is_program_running = self.is_running;
        if !self.is_running {
            self.machine.sync_insert_mode();
            self.machine.is_cursor_visible = true;
        }
    }
}
