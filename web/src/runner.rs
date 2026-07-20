//! `<canvas>` へ白黒画面をそのまま転送しながら VM を駆動する受動ランナー本体。
//!
//! 実行状態機械 (REPL/RUN/INPUT/WAIT) は core の [`Session`] が持ち、ここは
//! ブラウザイベントの変換・canvas 描画・JS コールバックだけを担う。

use ichigocrate_core::{
    ram::IJB_SIZEOF_ARRAY,
    render::{render_mono, RenderState, IMG_H, IMG_W},
    session::Session,
    BasicError, Machine,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

use crate::keymap::{code_to_btn_code, code_to_hid};
use crate::output::{detect_scroll, screen_char};
use crate::storage::WebStorage;

/// IchigoJam VM を 1 つ抱えるランナー。JS から `new IchigoCrateRunner(canvas)` で生成。
#[wasm_bindgen]
pub struct IchigoCrateRunner {
    /// 実行状態機械 (REPL/RUN/INPUT/WAIT) を持つ core 共通セッション。
    session: Session,
    ctx: CanvasRenderingContext2d,
    /// 使い回す 1bpp バッファ (0=消灯 1=点灯)。
    mono: Vec<u8>,
    /// 使い回す RGBA バッファ (canvas へ転送)。
    rgba: Vec<u8>,
    /// 起動時刻 (ms)。カーソル点滅位相の基準。`None` は初回 tick 未到達。
    start_ms: Option<f64>,
    /// onPrint コールバック (画面出力ストリーミング)。未登録なら差分監視も行わない。
    on_print: Option<js_sys::Function>,
    /// onError コールバック (実行時エラー通知)。即時文・RUN 中の停止理由を構造化して流す。
    on_error: Option<js_sys::Function>,
    /// onPrint 差分検出用: 直前フレームの VRAM スナップショット。
    prev_vram: Vec<u8>,
    /// onPrint で出力済みの位置 (出力カーソル列・行)。ここから現在のカーソルまでが新規。
    out_x: usize,
    out_y: usize,
}

#[wasm_bindgen]
impl IchigoCrateRunner {
    /// `canvas` を描画先に紐付けてランナーを生成する。canvas の解像度は論理
    /// 画面サイズ (IMG_W×IMG_H) に設定し、拡大表示は CSS 側に委ねる。
    ///
    /// `storage_prefix` は SAVE/LOAD/FILES の localStorage キー接頭辞 (複数
    /// インスタンスのスロット分離用、既定 "")。`persist` が false なら永続化せず
    /// セッション内のみ有効な揮発ストレージになる (既定 true)。
    #[wasm_bindgen(constructor)]
    pub fn new(
        canvas: &HtmlCanvasElement,
        storage_prefix: Option<String>,
        persist: Option<bool>,
    ) -> Result<IchigoCrateRunner, JsValue> {
        console_error_panic_hook::set_once();

        canvas.set_width(IMG_W as u32);
        canvas.set_height(IMG_H as u32);
        let ctx = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("2d context unavailable"))?
            .dyn_into::<CanvasRenderingContext2d>()?;

        let mut machine = Machine::new();
        machine.set_storage(Box::new(WebStorage::new(
            storage_prefix.unwrap_or_default(),
            persist.unwrap_or(true),
        )));
        let session = Session::new(machine);

        // onPrint の差分基準を現在 (起動バナー直後) に合わせ、バナーは流さない。
        let prev_vram = session.machine.vram().to_vec();
        let out_x = session.machine.cursorx.max(0) as usize;
        let out_y = session.machine.cursory.max(0) as usize;

        Ok(IchigoCrateRunner {
            session,
            ctx,
            mono: vec![0; IMG_W * IMG_H],
            rgba: vec![0; IMG_W * IMG_H * 4],
            start_ms: None,
            on_print: None,
            on_error: None,
            prev_vram,
            out_x,
            out_y,
        })
    }

    /// 1 フレーム進めて再描画する。`now_ms` は `performance.now()` を渡す。
    pub fn tick(&mut self, now_ms: f64) {
        if self.start_ms.is_none() {
            self.start_ms = Some(now_ms);
        }

        if let Some(e) = self.session.tick(now_ms) {
            self.report_error(e);
        }

        self.collect_output();

        let blink = ((now_ms - self.start_ms.unwrap_or(now_ms)) / 333.0) as u32;
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
            self.session.machine.key_set_down(btn, pressed);
        }
        if !pressed {
            return;
        }

        // ホスト側で別処理するキー (keymap には流さない)。状態に応じた
        // 振り分けは Session が担う。
        match code {
            "Enter" | "NumpadEnter" => {
                if let Some(e) = self.session.on_enter() {
                    self.report_error(e);
                }
                return;
            }
            "Escape" => {
                self.session.on_escape();
                return;
            }
            "F10" => {
                self.session.machine.toggle_kana();
                return;
            }
            _ => {}
        }

        // F1-F9 コマンド割当。受理条件 (REPL 待機中のみ) は Session が判定する。
        if let Some(n) = fkey_number(code) {
            if let Some(e) = self.session.press_fkey(n) {
                self.report_error(e);
            }
            return;
        }

        let Some(hid) = code_to_hid(code, self.session.machine.keyboard_id()) else {
            return;
        };
        let mut c = self.session.machine.keymap_lookup(hid, shift, alt);
        if c == 0 {
            return;
        }
        // IchigoJam 慣習: 英字は常に大文字 (CAPS デフォルト ON)。
        if c.is_ascii_lowercase() {
            c -= b'a' - b'A';
        }
        if let Some(e) = self.session.feed_char(c) {
            self.report_error(e);
        }
    }

    /// 現在カナモードか (タイトル表示などに使う)。
    pub fn is_kana(&self) -> bool {
        self.session.machine.is_kana_mode
    }

    /// LED が点灯中か (`LED 1` で true)。実機 LED の代わりにフロント側が画面枠を
    /// 赤くするなどの表示に使う (枠描画はフロントの責務)。
    pub fn is_led(&self) -> bool {
        self.session.machine.is_led_on
    }
}

/// 外部制御 API (`IchigoCrateHandle`)。
///
/// ブラウザからの直接キー入力に加え、JS/TS から入力・実行・状態取得を行うための
/// 命令インターフェイス。すべて [`IchigoCrateRunner`] のメソッドとして公開し、内部で
/// `core` の公開関数へ委譲する。React ラッパはこの面を `IchigoCrateHandle` という
/// ref 型として露出する。
///
/// **実行モデルの制約:** プログラムは無限ループが常態なので「`exec()` の戻りで完了を
/// 待つ」設計は採らない。`exec`/`run`/`loadProgram` は **停止中 (REPL) のみ受理**し、
/// 実行中は `type`/`keyDown`/`break` だけが有効 (フレーム途中に割り込まない)。
#[wasm_bindgen]
impl IchigoCrateRunner {
    /// 文字列をタイプ入力する (キーボード入力と同等)。実行中は INKEY()/INPUT へ、
    /// 停止中は REPL 行編集へ流れる。ASCII 以外の文字は無視する (グラフィック文字を
    /// 流したいときは将来の bytes 版を使う想定)。
    #[wasm_bindgen(js_name = "type")]
    pub fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            let u = ch as u32;
            if u < 0x80 {
                if let Some(e) = self.session.feed_char(u as u8) {
                    self.report_error(e);
                }
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

    /// 実機の RESET ボタン (電源 ON/OFF による再起動) 相当。LED・画面・
    /// カナ入力・VIDEO 設定なども含めて丸ごと起動直後の状態へ戻る。
    #[wasm_bindgen(js_name = "reset")]
    pub fn reset(&mut self) {
        self.session.reset();

        // onPrint の差分基準を現在 (起動バナー直後) に合わせ、バナーは流さない (new() と同じ扱い)。
        self.prev_vram = self.session.machine.vram().to_vec();
        self.out_x = self.session.machine.cursorx.max(0) as usize;
        self.out_y = self.session.machine.cursory.max(0) as usize;
    }

    /// INKEY()/BTN() 用の物理キー押下。`code` は IchigoJam キーコード
    /// (例: 28=←, 32=スペース, 88='X')。
    #[wasm_bindgen(js_name = "keyDown")]
    pub fn key_down(&mut self, code: u8) {
        self.session.machine.key_set_down(code, true);
    }

    /// INKEY()/BTN() 用の物理キー解放。
    #[wasm_bindgen(js_name = "keyUp")]
    pub fn key_up(&mut self, code: u8) {
        self.session.machine.key_set_down(code, false);
    }

    /// 実行中プログラムを中断する (ESC 相当)。暴走停止に使う。
    #[wasm_bindgen(js_name = "break")]
    pub fn break_(&mut self) {
        self.session.break_program();
    }

    /// 画面 (VRAM) を文字列スナップショットとして取得する。各行の末尾空白は
    /// 詰め、行は改行で連結する。印字不能・グラフィック文字は `?` に潰す。
    #[wasm_bindgen(js_name = "getScreenText")]
    pub fn get_screen_text(&self) -> String {
        let cols = self.session.machine.screen_cols();
        let rows = self.session.machine.screen_rows();
        let vram = self.session.machine.vram();

        let mut out = String::new();
        for y in 0..rows {
            let row = &vram[y * cols..(y + 1) * cols];
            let line: String = row.iter().map(|&c| screen_char(c)).collect();
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
            self.session
                .machine
                .var_get(IJB_SIZEOF_ARRAY + (up - b'A') as usize)
        } else {
            0
        }
    }

    /// メモリ (PEEK 相当) を読む。
    #[wasm_bindgen(js_name = "peek")]
    pub fn peek(&self, addr: i32) -> u8 {
        self.session.machine.peek(addr)
    }

    /// 画面出力ストリーミングのコールバックを登録する。`cb(chunk: string)` が
    /// フレームごとに新規出力分を受け取る (PRINT 出力・OK・キー入力エコーを含む
    /// 画面出力ストリーム)。`null`/未指定で解除。
    ///
    /// 実装は core を改変せず VRAM 差分で近似するため、1 フレーム内に画面外へ
    /// スクロールし切った行や、LOCATE 等でカーソルを戻して上書きした出力は
    /// 取りこぼすことがある。確実な全画面状態は [`get_screen_text`] を併用する。
    #[wasm_bindgen(js_name = "onPrint")]
    pub fn on_print(&mut self, cb: Option<js_sys::Function>) {
        self.on_print = cb;
    }

    /// 実行時エラーのコールバックを登録する。`cb({ code, message })` が、即時文
    /// (`exec`/`type` の改行確定) または RUN 中のプログラムが停止理由付きで止まった
    /// ときに呼ばれる。`code` は IchigoJam 標準のエラー番号 (1..=12)、`message` は
    /// 画面表示と同じ文言。`null`/未指定で解除。
    ///
    /// ESC=Break による中断は意図的操作なのでエラーとしては通知しない (画面には
    /// 従来どおり `Break in NN` が出る)。
    #[wasm_bindgen(js_name = "onError")]
    pub fn on_error(&mut self, cb: Option<js_sys::Function>) {
        self.on_error = cb;
    }
}

/// `KeyboardEvent.code` の `"F1"`-`"F9"` を F キー番号 1-9 へ変換する。
fn fkey_number(code: &str) -> Option<u8> {
    let n = code.strip_prefix('F')?.parse::<u8>().ok()?;
    (1..=9).contains(&n).then_some(n)
}

impl IchigoCrateRunner {
    /// REPL 1 行を直接実行し、エラーがあれば onError へ流す。
    fn exec_line_str(&mut self, line: &str) {
        if let Some(e) = self.session.exec_line(line.as_bytes()) {
            self.report_error(e);
        }
    }

    /// 画面出力の差分を抽出して onPrint コールバックへ流す (登録時のみ)。
    ///
    /// 直前フレームからの VRAM スクロール量を補正し、追跡中の出力位置から現在の
    /// カーソルまでを行単位で取り出す。core を一切改変しない近似実装。
    fn collect_output(&mut self) {
        let Some(cb) = self.on_print.clone() else {
            return;
        };
        let cols = self.session.machine.screen_cols();
        let rows = self.session.machine.screen_rows();
        let vram = self.session.machine.vram().to_vec();
        let cx = (self.session.machine.cursorx.max(0) as usize).min(cols);
        let cy = (self.session.machine.cursory.max(0) as usize).min(rows.saturating_sub(1));

        let scroll = detect_scroll(&self.prev_vram, &vram, cols, rows);
        if scroll > 0 {
            self.out_y = self.out_y.saturating_sub(scroll);
        }

        let mut chunk = String::new();
        if (self.out_y, self.out_x) < (cy, cx) {
            let mut y = self.out_y;
            while y <= cy {
                let start = if y == self.out_y { self.out_x } else { 0 };
                let end = if y == cy { cx } else { cols };
                let row = &vram[y * cols..y * cols + cols];
                let mut line: String = row[start..end].iter().map(|&c| screen_char(c)).collect();
                if y < cy {
                    // 行末空白を詰めて改行 (折返しで満杯の行は詰まらない)。
                    line.truncate(line.trim_end().len());
                    line.push('\n');
                }
                chunk.push_str(&line);
                y += 1;
            }
        }

        self.out_x = cx;
        self.out_y = cy;
        self.prev_vram = vram;

        if !chunk.is_empty() {
            let _ = cb.call1(&JsValue::NULL, &JsValue::from_str(&chunk));
        }
    }

    /// 停止理由を構造化して onError へ流す (登録時のみ)。`Break` は意図的中断なので
    /// 通知しない。
    fn report_error(&self, e: BasicError) {
        if e == BasicError::Break {
            return;
        }
        let Some(cb) = self.on_error.clone() else {
            return;
        };
        let obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&obj, &"code".into(), &JsValue::from(e.code()));
        let _ = js_sys::Reflect::set(&obj, &"message".into(), &JsValue::from_str(&e.to_string()));
        let _ = cb.call1(&JsValue::NULL, &obj);
    }

    fn render(&mut self, blink_phase: u32) {
        let state = RenderState::capture(&self.session.machine, blink_phase);
        render_mono(&mut self.mono, &self.session.machine, &state);
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
}
