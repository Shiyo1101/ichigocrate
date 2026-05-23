//! IchigoJam-RS デスクトップフロントエンド (egui + cpal)
//!
//! - VRAM を画像化して描画
//! - キーボード入力を IchigoJam の制御コードに変換してマシンに渡す
//! - PSG の現在周波数を矩形波として cpal で再生

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

/// IchigoJam の論理 1 フレーム = 1/60 秒
const FRAME: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// アイドル時 (REPL 待機) の再描画間隔。この状態で変化しうるのはカーソル
/// 点滅 (約 333ms 周期) だけなので、60Hz ではなくこの低頻度で再描画を
/// 要求し、待機中の CPU/GPU 消費を抑える。
const IDLE_REPAINT: Duration = Duration::from_millis(333);

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eframe::{egui, App, CreationContext};
use egui::{Color32, ColorImage, Key, TextureHandle, TextureOptions, Vec2};

use ichigojam_core::{
    exec_line, keycodes as kc,
    machine::{BasicResult, Storage, PC_NULL},
    LineOutcome, Machine, OFFSET_RAM_VRAM, SCREEN_H, SCREEN_W, SIZE_RAM_VRAM,
};

const PIXEL_SCALE: usize = 3;
const FONT_W: usize = 8;
const FONT_H: usize = 8;
const IMG_W: usize = SCREEN_W * FONT_W;
const IMG_H: usize = SCREEN_H * FONT_H;

/// IchigoJam 標準準拠の F キー割当て。3 番目の bool は「Enter まで自動投入するか」。
const FKEY_BINDINGS: &[(Key, &str, bool)] = &[
    (Key::F1, "CLS", true),
    (Key::F2, "LOAD", false),
    (Key::F3, "SAVE", false),
    (Key::F4, "LIST", true),
    (Key::F5, "RUN", true),
    (Key::F6, "?FREE()", true),
    (Key::F7, "?VER()", true),
    (Key::F8, "VIDEO", false),
    (Key::F9, "FILES", true),
];

/// egui の `Key` から REPL/エディタが扱う制御コードへのマップ。
const KEY_CONTROL_MAP: &[(Key, u8)] = &[
    (Key::Backspace, kc::BACKSPACE),
    (Key::Delete, kc::DELETE),
    (Key::ArrowLeft, kc::CURSOR_LEFT),
    (Key::ArrowRight, kc::CURSOR_RIGHT),
    (Key::ArrowUp, kc::CURSOR_UP),
    (Key::ArrowDown, kc::CURSOR_DOWN),
    (Key::Tab, kc::TAB),
    (Key::Home, kc::HOME),
    (Key::End, kc::END),
    (Key::PageUp, kc::PAGE_UP),
    (Key::PageDown, kc::PAGE_DOWN),
];

fn main() -> eframe::Result<()> {
    // macOS の Input Method Kit が出す
    // "error messaging the mach port for IMKCFRunLoopWakeUpReliable"
    // を抑制する (cpal/winit と macOS Sequoia の組合せで発生する無害なログ)
    #[cfg(target_os = "macos")]
    {
        // SAFETY: シングルスレッドの起動初期で他スレッドは未生成
        unsafe {
            std::env::set_var("OS_ACTIVITY_MODE", "disable");
        }
        filter_macos_stderr();
    }

    let shared_tone = Arc::new(AtomicU32::new(0));
    let _audio = match start_audio(shared_tone.clone()) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("[ichigojam] audio disabled: {e}");
            None
        }
    };

    let native_opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([
                (IMG_W * PIXEL_SCALE) as f32 + 32.0,
                (IMG_H * PIXEL_SCALE) as f32 + 32.0,
            ])
            .with_title("IchigoJam BASIC (Rust port)"),
        ..Default::default()
    };
    eframe::run_native(
        "IchigoJam BASIC",
        native_opts,
        Box::new(move |cc| Ok(Box::new(IchigoApp::new(cc, shared_tone, _audio)))),
    )
}

// macOS の IMK は AppKit のテキスト入力初期化時に
// "error messaging the mach port for IMKCFRunLoopWakeUpReliable"
// を NSLog で吐く。アプリの動作には影響しないが見栄えが悪いので、
// stderr を pipe してフィルタしたものを真の stderr に書き戻す。

#[cfg(target_os = "macos")]
fn filter_macos_stderr() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    let real_stderr_fd = unsafe { libc::dup(libc::STDERR_FILENO) };
    if real_stderr_fd < 0 {
        return;
    }
    let real_stderr = unsafe { OwnedFd::from_raw_fd(real_stderr_fd) };

    // stderr をパイプの書き込み側に張替える
    let mut fds = [0i32; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } < 0 {
        return;
    }
    let read_fd = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let write_fd = unsafe { OwnedFd::from_raw_fd(fds[1]) };
    if unsafe { libc::dup2(write_fd.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
        return;
    }
    drop(write_fd); // STDERR_FILENO に複製済み

    std::thread::spawn(move || {
        let reader = BufReader::new(std::fs::File::from(read_fd));
        let mut out = std::fs::File::from(real_stderr);
        for line in reader.lines().map_while(|r| r.ok()) {
            // 既知の無害な macOS IMK ログをスキップ
            if line.contains("IMKCFRunLoopWakeUpReliable")
                || line.contains("IMK")
            {
                continue;
            }
            let _ = writeln!(out, "{line}");
        }
    });
}

// 同 API を非 macOS でも参照できるよう no-op を用意
#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
fn filter_macos_stderr() {}

/// `~/.ichigojam-rs/slot_NN.ijb` にスロット単位で SAVE/LOAD する実装。
#[derive(Debug)]
struct DiskStorage {
    dir: PathBuf,
    slot_count: u8,
}

impl DiskStorage {
    fn new() -> Self {
        let dir = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ichigojam-rs");
        let _ = std::fs::create_dir_all(&dir);
        eprintln!("[ichigojam] file storage: {}", dir.display());
        Self { dir, slot_count: 16 }
    }

    fn path(&self, slot: u8) -> PathBuf {
        self.dir.join(format!("slot_{:02}.ijb", slot))
    }
}

impl Storage for DiskStorage {
    fn save(&mut self, slot: u8, data: &[u8]) -> bool {
        std::fs::write(self.path(slot), data).is_ok()
    }

    fn load(&mut self, slot: u8, buf: &mut [u8]) -> Option<usize> {
        let bytes = std::fs::read(self.path(slot)).ok()?;
        let n = bytes.len().min(buf.len());
        buf[..n].copy_from_slice(&bytes[..n]);
        // 残りはゼロ埋め (リストの終端を保証)
        buf[n..].fill(0);
        Some(n)
    }

    fn peek(&mut self, slot: u8, buf: &mut [u8]) -> Option<usize> {
        self.load(slot, buf)
    }

    fn slot_count(&self) -> u8 {
        self.slot_count
    }
}

struct IchigoApp {
    machine: Machine,
    /// VRAM → テクスチャ描画器 (バッファ使い回し + 変化時のみ再描画)
    renderer: Renderer,
    /// 実行中フラグ (program_running)
    running: bool,
    /// 音声共有
    shared_tone: Arc<AtomicU32>,
    _audio_stream: Option<cpal::Stream>,
    /// 起動時刻 (カーソル点滅などの基準)
    start_time: Instant,
    /// 次に 60Hz tick (PSG 等) を駆動する時刻
    next_tick_time: Instant,
    /// WAIT 終了予定時刻 (実時間ベース)
    wait_until: Option<Instant>,
    /// 直前フレームのカナモード状態 (タイトル更新差分用)
    last_kana: bool,
}

impl IchigoApp {
    fn new(
        _cc: &CreationContext<'_>,
        shared_tone: Arc<AtomicU32>,
        audio_stream: Option<cpal::Stream>,
    ) -> Self {
        let mut machine = Machine::new();
        machine.set_storage(Box::new(DiskStorage::new()));
        // タイトル表示
        for c in "IchigoJam BASIC 1.4 (Rust port)\n".bytes() {
            machine.put_chr(c);
        }
        for c in "OK\n".bytes() {
            machine.put_chr(c);
        }
        let now = Instant::now();
        Self {
            machine,
            renderer: Renderer::new(),
            running: false,
            shared_tone,
            _audio_stream: audio_stream,
            start_time: now,
            next_tick_time: now,
            wait_until: None,
            last_kana: false,
        }
    }
}

impl App for IchigoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();

        // 60Hz tick (PSG / frames) を実時間に同期して必要回数だけ進める
        let mut tick_iterations = 0;
        while self.next_tick_time <= now && tick_iterations < 8 {
            self.machine.frames = self.machine.frames.wrapping_add(1);
            self.machine.psg_tick();
            self.next_tick_time += FRAME;
            tick_iterations += 1;
        }
        // 大きく遅れた場合は追いつくのを諦めて基準時刻をリセット
        if self.next_tick_time + FRAME * 8 < now {
            self.next_tick_time = now;
        }

        self.sync_machine_before_input();
        process_keyboard(ctx, &mut self.machine);

        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.machine.key_flg_esc = 1;
        }

        // F キー: run=true は ENTER まで自動投入、false は文字挿入のみ
        // (ユーザが続けてスロット番号等を入力できるよう待機)
        if !self.running {
            let fkey = ctx.input(|i| {
                FKEY_BINDINGS
                    .iter()
                    .find(|(k, _, _)| i.key_pressed(*k))
                    .map(|(_, cmd, run)| (*cmd, *run))
            });
            if let Some((cmd, run)) = fkey {
                self.type_fkey_command(cmd, run);
            }
        }

        // WAIT 期限チェック (実時間ベース)
        if let Some(deadline) = self.wait_until {
            if now >= deadline {
                self.wait_until = None;
            }
        }
        // BASIC 側で WAIT が積まれていたら実時間の期限に変換
        if self.machine.wait_frames > 0 {
            let extra = FRAME * self.machine.wait_frames;
            let base = self.wait_until.unwrap_or(now);
            self.wait_until = Some(base + extra);
            self.machine.wait_frames = 0;
        }

        if self.running {
            // WAIT 中は basic_step を呼ばず実時間の経過を待つ。それ以外は
            // 1 フレームあたり最大 N 文まで進めて UI 凍結を防ぐ。
            if self.wait_until.is_none() {
                const MAX_STEPS_PER_FRAME: usize = 2000;
                for _ in 0..MAX_STEPS_PER_FRAME {
                    if self.machine.wait_frames > 0 {
                        break; // ステップ中に WAIT が発火 → 次フレームへ
                    }
                    if let Some(res) = self.machine.basic_step() {
                        self.running = false;
                        if res == BasicResult::Execute {
                            self.machine.put_str("OK\n");
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
        } else if ctx.input(|i| i.key_pressed(Key::Enter)) {
            self.execute_current_line();
        }

        self.shared_tone
            .store(self.machine.current_tone_hz.to_bits(), Ordering::Relaxed);

        // 再描画頻度をマシンの状態で決める。プログラム実行中・発音中・WAIT
        // 待機中は時間進行が必要なので 60Hz、それ以外の REPL 待機中はカーソル
        // 点滅に追従できれば十分なので低頻度に落とす (入力時は egui が自動で
        // 再描画するため取りこぼしはない)。ProMotion 等の高リフレッシュレート
        // でもこの間隔が再描画の上限になる。
        let needs_realtime =
            self.running || self.machine.psg_sound() || self.wait_until.is_some();
        ctx.request_repaint_after(if needs_realtime { FRAME } else { IDLE_REPAINT });

        // カナモードをウィンドウタイトルに反映
        if self.machine.key_kana != self.last_kana {
            let title = if self.machine.key_kana {
                "IchigoJam BASIC (Rust port) — KANA"
            } else {
                "IchigoJam BASIC (Rust port)"
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.to_string()));
            self.last_kana = self.machine.key_kana;
        }

        // カーソル点滅は実時間 (~333ms 周期) で算出 (リフレッシュ非依存)
        let cursor_blink_phase =
            ((now - self.start_time).as_millis() / 333) as u32;
        self.renderer.sync(ctx, &self.machine, cursor_blink_phase);

        // 実機 LED の代替: LED 1 で赤い枠、LED 0 で透明
        let border_color = if self.machine.led {
            Color32::from_rgb(230, 40, 40)
        } else {
            Color32::TRANSPARENT
        };

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(Color32::BLACK))
            .show(ctx, |ui| {
                if let Some(tex) = &self.renderer.texture {
                    let size = Vec2::new(
                        (IMG_W * PIXEL_SCALE) as f32,
                        (IMG_H * PIXEL_SCALE) as f32,
                    );
                    ui.centered_and_justified(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(size, egui::Sense::hover());
                        ui.painter().image(
                            tex.id(),
                            rect,
                            egui::Rect::from_min_max(
                                egui::pos2(0.0, 0.0),
                                egui::pos2(1.0, 1.0),
                            ),
                            Color32::WHITE,
                        );
                        ui.painter().rect_stroke(
                            rect.expand(8.0),
                            0.0,
                            egui::Stroke::new(8.0, border_color),
                        );
                    });
                }
            });
    }
}

impl IchigoApp {
    /// キーボード入力を処理する前に、マシン側の状態をフレームの実行状況へ
    /// 同期する。
    ///
    /// `program_running` を立てることで `input_putc`/`input_control` が実行中の
    /// 対話編集を無視する。判定に `pc` を使えないのが要点で、`pc` は STOP/ESC
    /// ブレーク後も CONT 用に残るため、停止しても入力が復活しなくなってしまう。
    /// 非実行 (REPL) 中は挿入/上書きモードを同期しカーソルを表示する
    /// (元 C 版 REPL の `screen_showCursor(1)` 相当)。
    fn sync_machine_before_input(&mut self) {
        self.machine.program_running = self.running;
        if !self.running {
            self.machine.sync_insert_mode();
            self.machine.cursorflg = true;
        }
    }

    /// F キーで指定コマンドを VRAM に挿入する。`run` が true なら直ちに実行、
    /// false ならカーソルだけ残し、ユーザが引数 (スロット番号など) を続けて
    /// 入力できるようにする。
    fn type_fkey_command(&mut self, cmd: &str, run: bool) {
        for b in cmd.bytes() {
            self.machine.screen_putc(b);
        }
        if run {
            self.execute_current_line();
        }
    }

    fn execute_current_line(&mut self) {
        // 元 C と同様、ENTER 押下時に改行を VRAM へ書き込む
        self.machine.screen_putc(b'\n');
        let p = self.machine.screen_gets();
        let mut line = String::new();
        let mut q = p;
        while q < OFFSET_RAM_VRAM + SIZE_RAM_VRAM {
            let c = self.machine.ram[q];
            if c == 0 {
                break;
            }
            line.push(c as char);
            q += 1;
        }
        if line.is_empty() {
            return;
        }
        self.machine.key_flg_esc = 0;
        match exec_line(&mut self.machine, &line) {
            Ok(LineOutcome::Executed) => {
                if self.machine.pc != PC_NULL {
                    // RUN 後など継続実行が必要
                    self.running = true;
                } else {
                    self.machine.put_str("OK\n");
                }
            }
            Ok(LineOutcome::Edited) => {
                // 行編集 (LIST 追加・削除) は OK を表示しない (IchigoJam 慣習)
            }
            Err(_err) => {
                // エラーメッセージは BASIC インタプリタ内で VRAM に
                // 書き済 (command_error → basic_print_error)。
            }
        }
    }
}

/// VRAM をテクスチャへ反映する描画器。
///
/// - **C (バッファ再利用)**: ピクセルバッファ `pixels` を保持して使い回し、
///   毎フレームの再確保 (旧 `vec![..]`) を排除する。
/// - **B (dirty 判定)**: 表示に影響する状態 (VRAM・PCG・[`Scalars`]) を前回分と
///   比較し、変化したフレームでだけ再描画と GPU への再アップロードを行う。
///   待機中は大半のフレームでこの処理を丸ごとスキップできる。
struct Renderer {
    /// 使い回す RGBA バッファ (IMG_W×IMG_H)。
    pixels: Vec<Color32>,
    texture: Option<TextureHandle>,
    /// 直前に描画した VRAM+PCG のスナップショット。
    last_vram_pcg: Vec<u8>,
    /// 直前に描画したスカラ状態。
    last_scalars: Option<Scalars>,
}

/// 描画結果に影響する VRAM/PCG 以外の状態。dirty 判定に使う。
#[derive(PartialEq)]
struct Scalars {
    invert: bool,
    video: bool,
    /// 拡大段階 (表示倍率は `1 << big`)。
    big: u8,
    /// 実際に反転描画されるカーソル: (セル index, 全角なら true)。
    /// 非表示 (点滅オフ・範囲外) のときは `None`。
    cursor: Option<(usize, bool)>,
}

impl Scalars {
    fn capture(m: &Machine, blink_phase: u32) -> Self {
        let cols = m.screen_cols();
        let rows = m.screen_rows();
        let show = m.cursorflg && (blink_phase & 1) == 0;
        let in_range = m.cursory >= 0
            && (m.cursory as usize) < rows
            && m.cursorx >= 0
            && (m.cursorx as usize) < cols;
        let cursor = if show && in_range {
            // 実機準拠: 上書きモードは文字全体、挿入モードは左半分のみ反転。
            Some((
                m.cursory as usize * cols + m.cursorx as usize,
                m.cursor_full_width(),
            ))
        } else {
            None
        };
        Self {
            invert: m.screen_invert,
            video: m.video_enabled,
            big: m.screen_big.min(3),
            cursor,
        }
    }
}

impl Renderer {
    fn new() -> Self {
        Self {
            pixels: vec![Color32::BLACK; IMG_W * IMG_H],
            texture: None,
            last_vram_pcg: Vec::new(),
            last_scalars: None,
        }
    }

    /// 表示状態が前回と変わっていればバッファを描き直し、テクスチャへ
    /// アップロードする。変化がなければ何もしない。
    fn sync(&mut self, ctx: &egui::Context, machine: &Machine, blink_phase: u32) {
        let scalars = Scalars::capture(machine, blink_phase);
        let vram = machine.vram();
        let pcg = machine.pcg();

        let unchanged = self.texture.is_some()
            && self.last_scalars.as_ref() == Some(&scalars)
            && self.last_vram_pcg.len() == vram.len() + pcg.len()
            && self.last_vram_pcg[..vram.len()] == *vram
            && self.last_vram_pcg[vram.len()..] == *pcg;
        if unchanged {
            return;
        }

        render_into(&mut self.pixels, machine, &scalars);

        // スナップショット更新 (確保は初回のみ、以降は容量を再利用)。
        self.last_vram_pcg.clear();
        self.last_vram_pcg.extend_from_slice(vram);
        self.last_vram_pcg.extend_from_slice(pcg);
        self.last_scalars = Some(scalars);

        let img = ColorImage {
            size: [IMG_W, IMG_H],
            pixels: self.pixels.clone(),
        };
        let opts = TextureOptions::NEAREST;
        match &mut self.texture {
            Some(tex) => tex.set(img, opts),
            None => self.texture = Some(ctx.load_texture("vram", img, opts)),
        }
    }
}

/// `Scalars` で確定済みの表示状態に従い、VRAM を `pixels` に描き込む。
///
/// VIDEO 3/4 の拡大表示では論理画面サイズ (cols×rows) が `SCREEN_W/H >> big`
/// に縮み、VRAM のストライドも cols になる。倍率 `zoom = 1 << big` を掛けると
/// `cols*zoom*FONT_W == IMG_W` となるため、可視領域をそのまま IMG_W×IMG_H へ
/// 引き伸ばせる。
fn render_into(pixels: &mut [Color32], machine: &Machine, s: &Scalars) {
    pixels.fill(Color32::BLACK);
    // VIDEO 0: 映像オフ。VRAM の内容に関わらず黒画面。
    if !s.video {
        return;
    }

    let vram = machine.vram();
    let pcg = machine.pcg();
    let zoom = 1usize << s.big as u32;
    let cols = machine.screen_cols();
    let rows = machine.screen_rows();

    for cy in 0..rows {
        for cx in 0..cols {
            let idx = cy * cols + cx;
            let ch = vram[idx];
            let glyph: &[u8] = if (0xE0..=0xFF).contains(&ch) {
                let p = (ch as usize - 0xE0) * 8;
                &pcg[p..p + 8]
            } else {
                let p = ch as usize * 8;
                &ichigojam_core::font::CHAR_PATTERN_JP[p..p + 8]
            };
            let cursor_here = matches!(s.cursor, Some((i, _)) if i == idx);
            let cursor_full = matches!(s.cursor, Some((i, full)) if i == idx && full);
            for (row, &bits) in glyph.iter().enumerate() {
                for col in 0..FONT_W {
                    let bit = (bits >> (7 - col)) & 1 != 0;
                    let mut on = bit;
                    if s.invert {
                        on = !on;
                    }
                    // カーソル反転。挿入モードは左半分 (col < 4) のみ反転して
                    // 細いカーソルにする (元 C の 0xf0 マスク相当)。
                    if cursor_here && (cursor_full || col < FONT_W / 2) {
                        on = !on;
                    }
                    let color = if on { Color32::WHITE } else { Color32::BLACK };
                    // 1 ソースピクセルを zoom×zoom ブロックに展開
                    let px0 = (cx * FONT_W + col) * zoom;
                    let py0 = (cy * FONT_H + row) * zoom;
                    for dy in 0..zoom {
                        for dx in 0..zoom {
                            pixels[(py0 + dy) * IMG_W + (px0 + dx)] = color;
                        }
                    }
                }
            }
        }
    }
}

fn process_keyboard(ctx: &egui::Context, m: &mut Machine) {
    ctx.input(|i| {
        // F10: ローマ字 → 半角カナ変換のオン/オフ
        // (本家 IchigoJam の Ctrl+Space は macOS では OS の入力ソース
        // 切替に予約されているため、両 OS で動く F10 を採用)
        if i.key_pressed(Key::F10) {
            m.toggle_kana();
        }
        // 元 C 版はキーバッファ (keybuf) が単一で、REPL 行編集 (IJB_input) も
        // INKEY() も同じ key_getKey() から読む。本移植は行編集を
        // input_putc/input_control が直接担うため、keybuf は INKEY() 専用に
        // なっている。よって全打鍵を keybuf にも流す必要があるが、REPL 編集
        // 中に積むと直接モードの INKEY() が編集文字を拾ってしまう (C 版は
        // 行エディタが keybuf を消費し実行開始時には空)。そこで実行中のみ
        // 積む。RUN は開始時に key_clear_key するのでこれで C と同じ挙動。
        let executing = m.is_executing();
        for ev in &i.events {
            if let egui::Event::Text(s) = ev {
                for c in s.chars() {
                    if let Some(b) = char_to_basic(c) {
                        // カナモード中はローマ字 → 半角カナ変換を通す
                        m.input_putc(b);
                        // INKEY() 用: 英字・数字・記号・スペースを取りこぼさない
                        if executing {
                            m.key_push(b);
                        }
                    }
                }
            }
        }
        for &(k, code) in KEY_CONTROL_MAP {
            if i.key_pressed(k) {
                // カナモード中の Backspace は未確定バッファ管理のため
                // input_putc を通す。それ以外の編集キーは input_control。
                if k == Key::Backspace && m.key_kana {
                    m.input_putc(code);
                } else {
                    m.input_control(code);
                }
                // INKEY() 用: 矢印 (28-31) や BS/DEL 等の制御コードも積む
                if executing {
                    m.key_push(code);
                }
            }
        }
        // Enter は Text イベントにも KEY_CONTROL_MAP にも現れないため個別に。
        // 非実行中は line 323 の REPL ハンドラが行確定に使う。
        if executing && i.key_pressed(Key::Enter) {
            m.key_push(b'\n');
        }
        // ウィンドウがフォーカスを失っている間は解放イベントを取りこぼし、
        // BTN() のキーが押しっぱなしになるため、押下状態を一括クリアする。
        if !i.focused {
            m.key_clear_down();
        }
        // BTN() 用の押下/解放状態を更新
        for ev in &i.events {
            if let egui::Event::Key { key, pressed, .. } = ev {
                if let Some(code) = key_to_btn_code(*key) {
                    m.key_set_down(code, *pressed);
                }
            }
        }
    });
}

/// egui の `Key` を BTN() が参照する ASCII コードへ変換する。
/// 矢印 (28-31) とスペース (32) を明示マップし、英字 A-Z / 数字 0-9 は
/// `Key::name()` (大文字 1 文字 / 数字 1 文字) からコードを得る。
/// 例: X キー → 88 ('X')。
fn key_to_btn_code(k: Key) -> Option<u8> {
    Some(match k {
        Key::ArrowLeft => kc::CURSOR_LEFT,
        Key::ArrowRight => kc::CURSOR_RIGHT,
        Key::ArrowUp => kc::CURSOR_UP,
        Key::ArrowDown => kc::CURSOR_DOWN,
        Key::Space => kc::SPACE,
        _ => {
            // 英字 A-Z / 数字 0-9 は Key::name() が 1 文字を返すのでその
            // ASCII をそのままコードにする (例: X キー → 'X' == 88)。
            let bytes = k.name().as_bytes();
            if bytes.len() == 1
                && (bytes[0].is_ascii_uppercase() || bytes[0].is_ascii_digit())
            {
                bytes[0]
            } else {
                return None;
            }
        }
    })
}

fn char_to_basic(c: char) -> Option<u8> {
    if c == '\r' || c == '\n' {
        return None; // ENTER は別ハンドラ
    }
    if (c as u32) >= 0x80 {
        return None;
    }
    let mut b = c as u8;
    // IchigoJam 慣習: 英字は常に大文字 (CAPS デフォルト ON)
    if b.is_ascii_lowercase() {
        b -= b'a' - b'A';
    }
    Some(b)
}

fn start_audio(tone: Arc<AtomicU32>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no output device")?;
    let config = device.default_output_config().map_err(|e| e.to_string())?;
    let sample_rate = config.sample_rate().0 as f32;
    let channels = config.channels() as usize;

    let mut phase: f32 = 0.0;
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| {
                    let hz = f32::from_bits(tone.load(Ordering::Relaxed));
                    let amp = 0.15f32;
                    for frame in data.chunks_mut(channels) {
                        let v = if hz <= 0.0 {
                            phase = 0.0;
                            0.0
                        } else {
                            let step = hz / sample_rate;
                            phase += step;
                            if phase >= 1.0 {
                                phase -= 1.0;
                            }
                            if phase < 0.5 {
                                amp
                            } else {
                                -amp
                            }
                        };
                        for s in frame.iter_mut() {
                            *s = v;
                        }
                    }
                },
                |err| eprintln!("audio error: {err}"),
                None,
            )
            .map_err(|e| e.to_string())?,
        fmt => return Err(format!("unsupported sample format: {fmt:?}")),
    };
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}
