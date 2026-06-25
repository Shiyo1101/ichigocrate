//! IchigoJam-RS の WebAssembly フロントエンド。
//!
//! eframe/egui は載せず、`ichigojam-core` の VM を直接駆動して `<canvas>` の 2D
//! コンテキストへ 1bpp 画面を blit する軽量ランナー。JS 側 (React ラッパや素の
//! HTML) が `requestAnimationFrame` ごとに [`IchigoJamRunner::tick`] を、キーイベント
//! ごとに [`IchigoJamRunner::on_key`] を呼ぶ受動オブジェクトとして設計する。これにより
//! 1 ページに複数インスタンスを貼ってもグローバル状態を共有しない。

use ichigojam_core::{
    exec_line, exec_line_bytes, keycodes as kc,
    ram::IJB_SIZEOF_ARRAY,
    render::{render_mono, RenderState, IMG_H, IMG_W},
    BasicResult, LineOutcome, Machine, OFFSET_RAM_VRAM, PC_NULL, SIZE_RAM_VRAM,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

/// IchigoJam の論理 1 フレーム = 1/60 秒 (ミリ秒)。
const FRAME_MS: f64 = 1000.0 / 60.0;
/// 1 フレームで進める最大文数 (UI 凍結防止。ネイティブ版と同値)。
const MAX_STEPS_PER_FRAME: usize = 2000;

/// IchigoJam VM を 1 つ抱えるランナー。JS から `new IchigoJamRunner(canvas)` で生成。
#[wasm_bindgen]
pub struct IchigoJamRunner {
    machine: Machine,
    ctx: CanvasRenderingContext2d,
    /// 使い回す 1bpp バッファ (0=消灯 1=点灯)。
    mono: Vec<u8>,
    /// 使い回す RGBA バッファ (canvas へ転送)。
    rgba: Vec<u8>,
    /// プログラム実行中フラグ (REPL 行確定や RUN で true)。
    running: bool,
    /// 60Hz tick を次に進める基準時刻 (ms)。
    next_tick_ms: f64,
    /// WAIT の実時間終了予定時刻 (ms)。
    wait_until_ms: Option<f64>,
    /// 起動時刻 (ms)。カーソル点滅位相の基準。
    start_ms: f64,
    /// INPUT 対話入力待ち中の値開始 VRAM 座標 (cursorx, cursory)。
    input_origin: Option<(i32, i32)>,
}

#[wasm_bindgen]
impl IchigoJamRunner {
    /// `canvas` を描画先に紐付けてランナーを生成する。canvas の解像度は論理
    /// 画面サイズ (IMG_W×IMG_H) に設定し、拡大表示は CSS 側に委ねる。
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: &HtmlCanvasElement) -> Result<IchigoJamRunner, JsValue> {
        console_error_panic_hook::set_once();

        canvas.set_width(IMG_W as u32);
        canvas.set_height(IMG_H as u32);
        let ctx = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("2d context unavailable"))?
            .dyn_into::<CanvasRenderingContext2d>()?;

        let mut machine = Machine::new();
        for c in "IchigoJam BASIC 1.4 (Rust port)\n".bytes() {
            machine.put_chr(c);
        }
        for c in "OK\n".bytes() {
            machine.put_chr(c);
        }

        Ok(IchigoJamRunner {
            machine,
            ctx,
            mono: vec![0; IMG_W * IMG_H],
            rgba: vec![0; IMG_W * IMG_H * 4],
            running: false,
            next_tick_ms: 0.0,
            wait_until_ms: None,
            start_ms: 0.0,
            input_origin: None,
        })
    }

    /// 1 フレーム進めて再描画する。`now_ms` は `performance.now()` を渡す。
    pub fn tick(&mut self, now_ms: f64) {
        // 初回呼び出しで時間基準を確定する。
        if self.start_ms == 0.0 {
            self.start_ms = now_ms;
            self.next_tick_ms = now_ms;
        }

        // 60Hz tick (PSG / frames カウンタ) を実時間に同期して必要回数進める。
        let mut iters = 0;
        while self.next_tick_ms <= now_ms && iters < 8 {
            self.machine.frames = self.machine.frames.wrapping_add(1);
            self.machine.psg_tick();
            self.next_tick_ms += FRAME_MS;
            iters += 1;
        }
        // 大きく遅れたら追いつくのを諦めて基準をリセット。
        if self.next_tick_ms + FRAME_MS * 8.0 < now_ms {
            self.next_tick_ms = now_ms;
        }

        // WAIT 期限チェック (期限到来で解除)。
        if let Some(deadline) = self.wait_until_ms {
            if now_ms >= deadline {
                self.wait_until_ms = None;
            }
        }
        // BASIC 側で積まれた WAIT を実時間の期限へ変換。
        if self.machine.wait_frames > 0 {
            let extra = FRAME_MS * self.machine.wait_frames as f64;
            let base = self.wait_until_ms.unwrap_or(now_ms);
            self.wait_until_ms = Some(base + extra);
            self.machine.wait_frames = 0;
        }

        self.machine.program_running = self.running;

        if self.running && self.wait_until_ms.is_none() {
            self.step_chunk();
        }

        // 待機 (REPL) 中はカーソルを表示し挿入モードを同期する。これを毎フレーム
        // 行わないと、コマンド実行後に次のキー入力まで点滅カーソルが出ない
        self.sync_before_input();

        let blink = ((now_ms - self.start_ms) / 333.0) as u32;
        self.render(blink);
    }

    /// キーイベントを 1 件処理する。`code` は `KeyboardEvent.code` (物理キー位置)、
    /// `shift`/`alt` は対応する修飾キー状態、`pressed` は keydown=true / keyup=false。
    ///
    /// 物理キー位置で keymap を引くため、`KBD` コマンドの US/JA 切替が OS の
    /// 入力レイアウトに依らず効く。
    pub fn on_key(&mut self, code: &str, shift: bool, alt: bool, pressed: bool) {
        // INKEY()/BTN() 用の押下状態は押下/解放の両方を反映する。
        if let Some(btn) = code_to_btn_code(code) {
            self.machine.key_set_down(btn, pressed);
        }
        if !pressed {
            return;
        }

        self.sync_before_input();
        let executing = self.machine.is_executing();

        // ホスト側で別処理するキー (keymap には流さない)。
        match code {
            "Enter" | "NumpadEnter" => {
                if executing {
                    self.machine.key_push(b'\n');
                } else if self.input_origin.is_some() {
                    self.complete_input();
                } else {
                    self.execute_current_line();
                }
                return;
            }
            "Escape" => {
                self.machine.key_flg_esc = 1;
                if self.input_origin.is_some() {
                    self.cancel_input();
                }
                return;
            }
            // F10: ローマ字 → 半角カナ変換のオン/オフ。
            "F10" => {
                self.machine.toggle_kana();
                return;
            }
            _ => {}
        }

        // F1-F9 コマンド割当 (REPL 待機中のみ。実行中/入力待ち中は無効)。
        if !self.running && self.input_origin.is_none() {
            if let Some((cmd, run)) = fkey_binding(code) {
                self.type_fkey_command(cmd, run);
                return;
            }
        }

        let Some(hid) = code_to_hid(code) else {
            return;
        };
        let mut c = self.machine.keymap_lookup(hid, shift, alt);
        if c == 0 {
            return;
        }
        // IchigoJam 慣習: 英字は常に大文字 (CAPS デフォルト ON)。
        if c.is_ascii_lowercase() {
            c -= b'a' - b'A';
        }
        self.feed_char(c);
    }

    /// 現在カナモードか (タイトル表示などに使う)。
    pub fn is_kana(&self) -> bool {
        self.machine.key_kana
    }

    /// LED が点灯中か (`LED 1` で true)。実機 LED の代わりにフロント側が画面枠を
    /// 赤くするなどの表示に使う (枠描画はフロントの責務)。
    pub fn is_led(&self) -> bool {
        self.machine.led
    }
}

/// 外部制御 API (`IchigoJamHandle`)。
///
/// ブラウザからの直接キー入力に加え、JS/TS から入力・実行・状態取得を行うための
/// 命令インターフェイス。すべて [`IchigoJamRunner`] のメソッドとして公開し、内部で
/// `core` の公開関数へ委譲する。React ラッパはこの面を `IchigoJamHandle` という
/// ref 型として露出する。
///
/// **実行モデルの制約:** プログラムは無限ループが常態なので「`exec()` の戻りで完了を
/// 待つ」設計は採らない。`exec`/`run`/`loadProgram` は **停止中 (REPL) のみ受理**し、
/// 実行中は `type`/`keyDown`/`stop` だけが有効 (フレーム途中に割り込まない)。
#[wasm_bindgen]
impl IchigoJamRunner {
    /// 文字列をタイプ入力する (キーボード入力と同等)。実行中は INKEY()/INPUT へ、
    /// 停止中は REPL 行編集へ流れる。ASCII 以外の文字は無視する (グラフィック文字を
    /// 流したいときは将来の bytes 版を使う想定)。
    #[wasm_bindgen(js_name = "type")]
    pub fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            let u = ch as u32;
            if u < 0x80 {
                self.feed_char(u as u8);
            }
        }
    }

    /// REPL の 1 行を直接実行する (画面エディタを介さない最もクリーンな経路)。
    /// 実行中・入力待ち中は無視される。
    #[wasm_bindgen(js_name = "exec")]
    pub fn exec(&mut self, line: &str) {
        self.exec_line_str(line);
    }

    /// 複数行をまとめて投入する (行番号付きは LIST 領域へ格納される)。
    #[wasm_bindgen(js_name = "loadProgram")]
    pub fn load_program(&mut self, text: &str) {
        for line in text.split(['\n', '\r']).filter(|l| !l.is_empty()) {
            self.exec_line_str(line);
        }
    }

    /// `RUN` 相当。格納済みプログラムの実行を開始する。
    #[wasm_bindgen(js_name = "run")]
    pub fn run(&mut self) {
        self.exec_line_str("RUN");
    }

    /// `basic_init` 相当。変数・プログラム・実行状態をリセットする。
    #[wasm_bindgen(js_name = "reset")]
    pub fn reset(&mut self) {
        self.machine.basic_init();
        self.running = false;
        self.input_origin = None;
        self.wait_until_ms = None;
    }

    /// INKEY()/BTN() 用の物理キー押下。`code` は IchigoJam キーコード
    /// (例: 28=←, 32=スペース, 88='X')。
    #[wasm_bindgen(js_name = "keyDown")]
    pub fn key_down(&mut self, code: u8) {
        self.machine.key_set_down(code, true);
    }

    /// INKEY()/BTN() 用の物理キー解放。
    #[wasm_bindgen(js_name = "keyUp")]
    pub fn key_up(&mut self, code: u8) {
        self.machine.key_set_down(code, false);
    }

    /// 実行中プログラムを中断する (ESC 相当)。暴走停止に使う。
    #[wasm_bindgen(js_name = "stop")]
    pub fn stop(&mut self) {
        self.machine.key_flg_esc = 1;
    }

    /// 画面 (VRAM) を文字列スナップショットとして取得する。各行の末尾空白は
    /// 詰め、行は改行で連結する。印字不能・グラフィック文字は `?` に潰す。
    #[wasm_bindgen(js_name = "getScreenText")]
    pub fn get_screen_text(&self) -> String {
        let cols = self.machine.screen_cols();
        let rows = self.machine.screen_rows();
        let vram = self.machine.vram();
        let mut out = String::new();
        for y in 0..rows {
            let row = &vram[y * cols..(y + 1) * cols];
            let line: String = row
                .iter()
                .map(|&c| match c {
                    0 => ' ',
                    32..=126 => c as char,
                    _ => '?',
                })
                .collect();
            out.push_str(line.trim_end());
            if y + 1 < rows {
                out.push('\n');
            }
        }
        out
    }

    /// 変数 A-Z の値を取得する (`name` の先頭 1 文字、大小無視)。
    #[wasm_bindgen(js_name = "getVar")]
    pub fn get_var(&self, name: &str) -> i16 {
        let Some(ch) = name.bytes().next() else {
            return 0;
        };
        let up = ch.to_ascii_uppercase();
        if up.is_ascii_uppercase() {
            self.machine.var_get(IJB_SIZEOF_ARRAY + (up - b'A') as usize)
        } else {
            0
        }
    }

    /// メモリ (PEEK 相当) を読む。
    #[wasm_bindgen(js_name = "peek")]
    pub fn peek(&self, addr: i32) -> u8 {
        self.machine.peek(addr)
    }
}

impl IchigoJamRunner {
    /// プログラム実行を 1 フレーム分 (最大 MAX_STEPS_PER_FRAME 文) 進める。
    /// 完了・INPUT 待ち・WAIT 発火で打ち切る。tick() と exec 系で共有する。
    fn step_chunk(&mut self) {
        for _ in 0..MAX_STEPS_PER_FRAME {
            if self.machine.wait_frames > 0 {
                break; // ステップ中に WAIT 発火 → 次フレームへ
            }
            if let Some(res) = self.machine.basic_step() {
                self.running = false;
                match res {
                    BasicResult::Execute => self.machine.put_str("OK\n"),
                    BasicResult::Input => self.begin_input(),
                    _ => {}
                }
                self.machine.key_flg_esc = 0;
                break;
            }
            if self.machine.pc == PC_NULL {
                self.running = false;
                self.machine.put_str("OK\n");
                break;
            }
        }
    }

    /// 解決済みの 1 文字をモード適応で流す (`on_key` と `type` の共通経路)。
    /// 実行中は keybuf (INKEY/INPUT) へ、停止中は REPL 行編集へ振り分ける。
    fn feed_char(&mut self, c: u8) {
        self.sync_before_input();
        if self.machine.is_executing() {
            self.machine.key_push(c);
            return;
        }
        match c {
            b'\n' | b'\r' => {
                if self.input_origin.is_some() {
                    self.complete_input();
                } else {
                    self.execute_current_line();
                }
            }
            // カナモード中の Backspace は未確定バッファ管理のため input_putc を通す。
            _ if is_edit_control_code(c) => {
                if c == kc::BACKSPACE && self.machine.key_kana {
                    self.machine.input_putc(c);
                } else {
                    self.machine.input_control(c);
                }
            }
            // グラフィック文字 (128-255) はローマ字 → カナ変換を通さない。
            _ if c >= 128 => self.machine.screen_putc(c),
            _ => self.machine.input_putc(c),
        }
    }

    /// REPL 1 行を直接実行する。停止中 (REPL) のみ受理し、実行中・入力待ち中は無視。
    fn exec_line_str(&mut self, line: &str) {
        if self.running || self.input_origin.is_some() {
            return;
        }
        self.machine.program_running = false;
        self.machine.key_flg_esc = 0;
        match exec_line(&mut self.machine, line) {
            Ok(LineOutcome::Executed) => self.finish_executed(),
            Ok(LineOutcome::Edited) => {}
            Ok(LineOutcome::AwaitingInput) => self.begin_input(),
            Err(_) => {}
        }
    }

    /// `exec_line`/`execute_current_line` が `Executed` を返した後の共通処理。
    ///
    /// IchigoJam は実行後も pc を非 NULL に残し後続の basic_step で完了する設計
    /// なので、即時文はここで 1 フレーム分まで同期実行して完了させる。終わらなければ
    /// (RUN の無限ループ等) running を立ててフレーム側へ委譲し、ブラウザを固めない。
    fn finish_executed(&mut self) {
        if self.machine.pc != PC_NULL {
            self.running = true;
            self.machine.program_running = true;
            if self.wait_until_ms.is_none() {
                self.step_chunk();
            }
        } else {
            self.machine.put_str("OK\n");
        }
    }

    /// キー入力処理の前にマシン状態をフレームの実行状況へ同期する。
    fn sync_before_input(&mut self) {
        self.machine.program_running = self.running;
        if !self.running {
            self.machine.sync_insert_mode();
            self.machine.cursorflg = true;
        }
    }

    /// 1bpp 画面を描いて canvas へ転送する。
    fn render(&mut self, blink_phase: u32) {
        let state = RenderState::capture(&self.machine, blink_phase);
        render_mono(&mut self.mono, &self.machine, &state);
        for (i, &on) in self.mono.iter().enumerate() {
            let v = if on != 0 { 255 } else { 0 };
            let p = i * 4;
            self.rgba[p] = v;
            self.rgba[p + 1] = v;
            self.rgba[p + 2] = v;
            self.rgba[p + 3] = 255;
        }
        if let Ok(img) = ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(&self.rgba),
            IMG_W as u32,
            IMG_H as u32,
        ) {
            let _ = self.ctx.put_image_data(&img, 0.0, 0.0);
        }
    }

    /// F キーで指定コマンドを VRAM に挿入する。`run` が true なら直ちに実行。
    fn type_fkey_command(&mut self, cmd: &str, run: bool) {
        for b in cmd.bytes() {
            self.machine.screen_putc(b);
        }
        if run {
            self.execute_current_line();
        }
    }

    /// Enter 押下時: 現在行を生バイト列として取り出し REPL 実行する。
    fn execute_current_line(&mut self) {
        self.machine.screen_putc(b'\n');
        let p = self.machine.screen_gets();
        // VRAM から行長を測り生バイトのスライスを得る (String 経由は 0x80-0xFF を
        // UTF-8 展開してしまうため不可)。
        let vram_end = OFFSET_RAM_VRAM + SIZE_RAM_VRAM;
        let len = self.machine.ram[p..vram_end]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(vram_end - p);
        if len == 0 {
            return;
        }
        self.machine.key_flg_esc = 0;
        let line: Vec<u8> = self.machine.ram[p..p + len].to_vec();
        match exec_line_bytes(&mut self.machine, &line) {
            Ok(LineOutcome::Executed) => self.finish_executed(),
            // 行編集 (LIST 追加・削除) は OK を表示しない (IchigoJam 慣習)。
            Ok(LineOutcome::Edited) => {}
            // 即時モードの INPUT。対話入力モードへ移行する。
            Ok(LineOutcome::AwaitingInput) => self.begin_input(),
            // エラーメッセージは VRAM に書き済 (basic_print_error)。
            Err(_) => {}
        }
    }

    /// INPUT 入力待ちの開始。プロンプト直後のカーソル位置を値の開始に記録する。
    fn begin_input(&mut self) {
        self.input_origin = Some((self.machine.cursorx, self.machine.cursory));
        self.machine.key_flg_esc = 0;
    }

    /// INPUT の入力確定。値テキストを読み取り変数へ反映して実行を再開する。
    fn complete_input(&mut self) {
        let (ox, oy) = self.input_origin.take().unwrap_or((0, 0));
        let w = self.machine.screen_cols();
        let start = OFFSET_RAM_VRAM + ox as usize + oy as usize * w;
        let vram_end = OFFSET_RAM_VRAM + SIZE_RAM_VRAM;
        let len = self.machine.ram[start..vram_end]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(vram_end - start);
        let line: Vec<u8> = self.machine.ram[start..start + len].to_vec();
        self.machine.input_complete(&line);
        self.running = true;
    }

    /// INPUT 入力中の ESC 中断。代入せず REPL へ戻る。
    fn cancel_input(&mut self) {
        self.input_origin = None;
        self.machine.cancel_input();
        self.machine.put_str("OK\n");
        self.machine.key_flg_esc = 0;
    }
}

/// keymap の戻り値のうち REPL 編集を進める制御コード群 (input_control 経由)。
fn is_edit_control_code(c: u8) -> bool {
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

/// F1-F9 のコマンド割当。3 番目は「Enter まで自動実行するか」。
fn fkey_binding(code: &str) -> Option<(&'static str, bool)> {
    Some(match code {
        "F1" => ("CLS", true),
        "F2" => ("LOAD", false),
        "F3" => ("SAVE", false),
        "F4" => ("LIST", true),
        "F5" => ("RUN", true),
        "F6" => ("?FREE()", true),
        "F7" => ("?VER()", true),
        "F8" => ("VIDEO", false),
        "F9" => ("FILES", true),
        _ => return None,
    })
}

/// `KeyboardEvent.code` を USB HID Keyboard Usage ID へ変換する。
/// 添字は HID Usage ID に一致させ (例: Digit2=0x1f、BracketLeft=0x2f)、
/// 物理キー位置で keymap を引いて KBD の US/JA 切替を OS 非依存にする入り口。
fn code_to_hid(code: &str) -> Option<u8> {
    Some(match code {
        // 英字: KeyA=0x04 … KeyZ=0x1d
        "KeyA" => 0x04,
        "KeyB" => 0x05,
        "KeyC" => 0x06,
        "KeyD" => 0x07,
        "KeyE" => 0x08,
        "KeyF" => 0x09,
        "KeyG" => 0x0a,
        "KeyH" => 0x0b,
        "KeyI" => 0x0c,
        "KeyJ" => 0x0d,
        "KeyK" => 0x0e,
        "KeyL" => 0x0f,
        "KeyM" => 0x10,
        "KeyN" => 0x11,
        "KeyO" => 0x12,
        "KeyP" => 0x13,
        "KeyQ" => 0x14,
        "KeyR" => 0x15,
        "KeyS" => 0x16,
        "KeyT" => 0x17,
        "KeyU" => 0x18,
        "KeyV" => 0x19,
        "KeyW" => 0x1a,
        "KeyX" => 0x1b,
        "KeyY" => 0x1c,
        "KeyZ" => 0x1d,
        // 数字行: Digit1=0x1e … Digit9=0x26、Digit0=0x27
        "Digit1" => 0x1e,
        "Digit2" => 0x1f,
        "Digit3" => 0x20,
        "Digit4" => 0x21,
        "Digit5" => 0x22,
        "Digit6" => 0x23,
        "Digit7" => 0x24,
        "Digit8" => 0x25,
        "Digit9" => 0x26,
        "Digit0" => 0x27,
        // 制御 + Space
        "Backspace" => 0x2a,
        "Tab" => 0x2b,
        "Space" => 0x2c,
        // 記号 (物理位置で引くため US 配列基準のキー名で対応)
        "Minus" => 0x2d,
        "Equal" => 0x2e,
        "BracketLeft" => 0x2f,
        "BracketRight" => 0x30,
        "Backslash" => 0x31,
        "Semicolon" => 0x33,
        "Quote" => 0x34,
        "Backquote" => 0x35,
        "Comma" => 0x36,
        "Period" => 0x37,
        "Slash" => 0x38,
        // カーソル / 編集系
        "Insert" => 0x49,
        "Home" => 0x4a,
        "PageUp" => 0x4b,
        "Delete" => 0x4c,
        "End" => 0x4d,
        "PageDown" => 0x4e,
        "ArrowRight" => 0x4f,
        "ArrowLeft" => 0x50,
        "ArrowDown" => 0x51,
        "ArrowUp" => 0x52,
        _ => return None,
    })
}

/// `KeyboardEvent.code` を BTN() が参照する ASCII コードへ変換する。
/// 矢印 (28-31) とスペース (32) を明示マップし、英字 KeyA-KeyZ / 数字
/// Digit0-9 は文字そのものの ASCII を使う (例: KeyX → 'X' == 88)。
fn code_to_btn_code(code: &str) -> Option<u8> {
    Some(match code {
        "ArrowLeft" => kc::CURSOR_LEFT,
        "ArrowRight" => kc::CURSOR_RIGHT,
        "ArrowUp" => kc::CURSOR_UP,
        "ArrowDown" => kc::CURSOR_DOWN,
        "Space" => kc::SPACE,
        _ => {
            if let Some(letter) = code.strip_prefix("Key") {
                let b = letter.as_bytes();
                if b.len() == 1 && b[0].is_ascii_uppercase() {
                    return Some(b[0]);
                }
            }
            if let Some(digit) = code.strip_prefix("Digit") {
                let b = digit.as_bytes();
                if b.len() == 1 && b[0].is_ascii_digit() {
                    return Some(b[0]);
                }
            }
            return None;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_to_hid_matches_physical_positions() {
        assert_eq!(code_to_hid("KeyA"), Some(0x04));
        assert_eq!(code_to_hid("KeyZ"), Some(0x1d));
        assert_eq!(code_to_hid("Digit2"), Some(0x1f));
        assert_eq!(code_to_hid("Digit0"), Some(0x27));
        assert_eq!(code_to_hid("BracketLeft"), Some(0x2f));
        assert_eq!(code_to_hid("ArrowLeft"), Some(0x50));
        assert_eq!(code_to_hid("Enter"), None);
    }

    #[test]
    fn btn_code_maps_letters_digits_and_arrows() {
        assert_eq!(code_to_btn_code("KeyX"), Some(b'X'));
        assert_eq!(code_to_btn_code("Digit0"), Some(b'0'));
        assert_eq!(code_to_btn_code("ArrowLeft"), Some(kc::CURSOR_LEFT));
        assert_eq!(code_to_btn_code("Space"), Some(kc::SPACE));
        assert_eq!(code_to_btn_code("Enter"), None);
    }
}
