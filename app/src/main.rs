//! IchigoCrate デスクトップフロントエンド (egui + cpal)
//!
//! - VRAM を画像化して描画
//! - キーボード入力を IchigoJam の制御コードに変換してマシンに渡す
//! - PSG の現在周波数を矩形波として cpal で再生

#![deny(unsafe_code)]

use std::path::PathBuf;
use std::sync::{atomic::Ordering, Arc};

use atomic_float::AtomicF32;
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

use ichigocrate_core::{
    exec_line_bytes, keycodes as kc,
    machine::{BasicResult, Storage, PC_NULL},
    render::{render_mono, RenderState, IMG_H, IMG_W},
    LineOutcome, Machine, OFFSET_RAM_VRAM, SIZE_RAM_VRAM,
};

const PIXEL_SCALE: usize = 3;

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


fn main() -> eframe::Result<()> {
    // macOS の Input Method Kit が出す
    // "error messaging the mach port for IMKCFRunLoopWakeUpReliable"
    // を抑制する (cpal/winit と macOS Sequoia の組合せで発生する無害なログ)
    #[cfg(target_os = "macos")]
    filter_macos_stderr();

    let shared_tone = Arc::new(AtomicF32::new(0.0));
    let _audio = match start_audio(shared_tone.clone()) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("[ichigocrate] audio disabled: {e}");
            None
        }
    };

    let native_opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([
                (IMG_W * PIXEL_SCALE) as f32 + 32.0,
                (IMG_H * PIXEL_SCALE) as f32 + 32.0,
            ])
            .with_title("IchigoCrate BASIC"),
        ..Default::default()
    };
    eframe::run_native(
        "IchigoCrate BASIC",
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

    // 真の stderr 書込みハンドルを保存 (fd 2 はパイプ側に張替える)
    let Ok(mut real_stderr) = os_pipe::dup_stderr() else { return };
    let Ok((reader, writer)) = os_pipe::pipe() else { return };
    if rustix::stdio::dup2_stderr(&writer).is_err() {
        return;
    }
    drop(writer);

    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            // 既知の無害な macOS IMK ログをスキップ
            if line.contains("IMK") {
                continue;
            }
            let _ = writeln!(real_stderr, "{line}");
        }
    });
}

// 同 API を非 macOS でも参照できるよう no-op を用意
#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
fn filter_macos_stderr() {}

/// `~/.ichigocrate/slot_NN.ijb` にスロット単位で SAVE/LOAD する実装。
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
            .join(".ichigocrate");
        let _ = std::fs::create_dir_all(&dir);
        eprintln!("[ichigocrate] file storage: {}", dir.display());
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
    /// VRAM → テクスチャ描画器 (バッファ使い回し + 差分判定で再描画を抑える)
    renderer: Renderer,
    /// Machine.is_program_running と同期するホスト側のミラー
    is_running: bool,
    shared_tone: Arc<AtomicF32>,
    _audio_stream: Option<cpal::Stream>,
    /// カーソル点滅の基準
    start_time: Instant,
    /// 次に 60Hz tick (PSG 等) を駆動する時刻
    next_tick_time: Instant,
    /// 実時間ベースの WAIT 終了予定時刻
    wait_until: Option<Instant>,
    /// タイトル更新差分用に持つ直前フレームのカナモード状態
    was_kana_mode: bool,
    /// INPUT 文の対話入力待ち中なら、入力値の開始 VRAM 座標 (プロンプト直後の
    /// cursorx, cursory)。Enter 確定時にこの位置から値テキストを読み取る。
    input_origin: Option<(i32, i32)>,
    /// 次回の `execute_current_line` で "OK" 表示を抑止するフラグ。F1 (CLS)
    /// ショートカットは画面を消すのが目的なので、直後に "OK" が出ると空白画面
    /// にならず UX を損なう。
    suppress_next_ok: bool,
}

impl IchigoApp {
    fn new(
        _cc: &CreationContext<'_>,
        shared_tone: Arc<AtomicF32>,
        audio_stream: Option<cpal::Stream>,
    ) -> Self {
        let mut machine = Machine::new();
        machine.set_storage(Box::new(DiskStorage::new()));
        machine.power_on_reset();
        machine.put_str("OK\n");
        let now = Instant::now();
        Self {
            machine,
            renderer: Renderer::new(),
            is_running: false,
            shared_tone,
            _audio_stream: audio_stream,
            start_time: now,
            next_tick_time: now,
            wait_until: None,
            was_kana_mode: false,
            input_origin: None,
            suppress_next_ok: false,
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
            self.machine.is_esc_pressed = true;
        }

        // F キー: run=true は ENTER まで自動投入、false は文字挿入のみ
        // (ユーザが続けてスロット番号等を入力できるよう待機)。
        // INPUT の入力待ち中は値編集を妨げないため無効化する。
        if !self.is_running && self.input_origin.is_none() {
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

        if self.is_running {
            // WAIT 中は basic_step を呼ばず実時間の経過を待つ。それ以外は
            // 1 フレームあたり最大 N 文まで進めて UI 凍結を防ぐ。
            if self.wait_until.is_none() {
                const MAX_STEPS_PER_FRAME: usize = 2000;
                for _ in 0..MAX_STEPS_PER_FRAME {
                    if self.machine.wait_frames > 0 {
                        break; // ステップ中に WAIT が発火 → 次フレームへ
                    }
                    if let Some(res) = self.machine.basic_step() {
                        self.is_running = false;
                        match res {
                            // 即時実行の完了はここが本当の到達点 (pc は PC_NULL に戻らず
                            // LINEBUF 内の終端を指したまま返ってくるため)。F1 (CLS) の
                            // "OK" 抑止もここで判定しないと効かない。
                            BasicResult::Execute => {
                                if !std::mem::take(&mut self.suppress_next_ok) {
                                    self.machine.put_str("OK\n");
                                }
                            }
                            // INPUT 文が入力待ち → 対話入力モードへ。実行再開は
                            // 入力確定時 (complete_input) に is_running を立て直す。
                            BasicResult::Input => self.begin_input(),
                            _ => {}
                        }
                        self.machine.is_esc_pressed = false;
                        break;
                    }
                    if self.machine.pc == PC_NULL {
                        self.is_running = false;
                        if !std::mem::take(&mut self.suppress_next_ok) {
                            self.machine.put_str("OK\n");
                        }
                        break;
                    }
                }
            }
        } else if self.input_origin.is_some() {
            // INPUT 入力待ち: ESC で中断、Enter で値を確定して実行再開。
            if self.machine.is_esc_pressed {
                self.cancel_input();
            } else if ctx.input(|i| i.key_pressed(Key::Enter)) {
                self.complete_input();
            }
        } else if ctx.input(|i| i.key_pressed(Key::Enter)) {
            self.execute_current_line();
        }

        self.shared_tone
            .store(self.machine.current_tone_hz, Ordering::Relaxed);

        // 再描画頻度をマシンの状態で決める。プログラム実行中・発音中・WAIT
        // 待機中は時間進行が必要なので 60Hz、それ以外の REPL 待機中はカーソル
        // 点滅に追従できれば十分なので低頻度に落とす (入力時は egui が自動で
        // 再描画するため取りこぼしはない)。ProMotion 等の高リフレッシュレート
        // でもこの間隔が再描画の上限になる。
        let needs_realtime =
            self.is_running || self.machine.psg_sound() || self.wait_until.is_some();
        ctx.request_repaint_after(if needs_realtime { FRAME } else { IDLE_REPAINT });

        if self.machine.is_kana_mode != self.was_kana_mode {
            let title = if self.machine.is_kana_mode {
                "IchigoCrate BASIC — KANA"
            } else {
                "IchigoCrate BASIC"
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.to_string()));
            self.was_kana_mode = self.machine.is_kana_mode;
        }

        // カーソル点滅は実時間 (~333ms 周期) で算出 (リフレッシュ非依存)
        let cursor_blink_phase =
            ((now - self.start_time).as_millis() / 333) as u32;
        self.renderer.sync(ctx, &self.machine, cursor_blink_phase);

        // 実機 LED の代替: LED 1 で赤い枠、LED 0 で透明
        let border_color = if self.machine.is_led_on {
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
    /// `is_program_running` を立てることで `input_putc`/`input_control` が実行中の
    /// 対話編集を無視する。判定に `pc` を使えないのが要点で、`pc` は STOP/ESC
    /// ブレーク後も CONT 用に残るため、停止しても入力が復活しなくなってしまう。
    /// 非実行 (REPL) 中は挿入/上書きモードを同期しカーソルを表示する。
    fn sync_machine_before_input(&mut self) {
        self.machine.is_program_running = self.is_running;
        if !self.is_running {
            self.machine.sync_insert_mode();
            self.machine.is_cursor_visible = true;
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
            // CLS 直後は画面を空白のまま保ちたいので "OK" 表示を抑止する。
            self.suppress_next_ok = cmd == "CLS";
            self.execute_current_line();
        }
    }

    fn execute_current_line(&mut self) {
        // ENTER 押下時の改行を VRAM へ書き込む
        self.machine.screen_putc(b'\n');
        let p = self.machine.screen_gets();
        // VRAM から行の長さを測り、生バイト列のスライスを取得する。String 経由
        // (`c as char` → `as_bytes()`) は 0x80-0xFF のグラフィック文字を UTF-8
        // で 2 バイトに展開してしまうため使えない。
        let vram_end = OFFSET_RAM_VRAM + SIZE_RAM_VRAM;
        let len = self.machine.ram[p..vram_end]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(vram_end - p);
        if len == 0 {
            return;
        }
        self.machine.is_esc_pressed = false;
        // Machine 借用のため一旦スライスをローカルにコピー
        let line: Vec<u8> = self.machine.ram[p..p + len].to_vec();
        match exec_line_bytes(&mut self.machine, &line) {
            Ok(LineOutcome::Executed) => {
                if self.machine.pc != PC_NULL {
                    // RUN 後など継続実行が必要。即時実行の完了もここを通る
                    // (pc は PC_NULL に戻らず LINEBUF の終端を指したまま返る
                    // ため)。実際の完了検知・"OK" 表示は update() の
                    // is_running 処理ループ側で行う。
                    self.is_running = true;
                } else if !std::mem::take(&mut self.suppress_next_ok) {
                    self.machine.put_str("OK\n");
                }
            }
            Ok(LineOutcome::Edited) => {
                // 行編集 (LIST 追加・削除) は OK を表示しない (IchigoJam 慣習)
            }
            Ok(LineOutcome::AwaitingInput) => {
                // 即時モードの INPUT。対話入力モードへ移行する。
                self.begin_input();
            }
            Err(_err) => {
                // エラーメッセージは BASIC インタプリタ内で VRAM に
                // 書き済 (command_error → basic_print_error)。
            }
        }
    }

    /// INPUT 文が入力待ちに入った時の準備。プロンプトは既に表示済みなので、
    /// 現在のカーソル位置 (プロンプト直後) を入力値の開始位置として記録する。
    fn begin_input(&mut self) {
        self.input_origin = Some((self.machine.cursorx, self.machine.cursory));
        self.machine.is_esc_pressed = false;
    }

    /// INPUT の入力確定。プロンプト直後から現在のカーソル行末までの VRAM を
    /// 値テキストとして読み取り、`input_complete` で変数へ反映して実行を再開する。
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
}

/// VRAM をテクスチャへ反映する描画器。
///
/// - **C (バッファ再利用)**: モノクロバッファ `mono` と色付け後の `pixels` を
///   保持して使い回し、毎フレームの再確保 (旧 `vec![..]`) を排除する。
/// - **B (dirty 判定)**: 表示に影響する状態 (VRAM・PCG・[`RenderState`]) を前回分と
///   比較し、変化したフレームでだけ再描画と GPU への再アップロードを行う。
///   待機中は大半のフレームでこの処理を丸ごとスキップできる。
///
/// VRAM→ビットマップの展開そのものは [`render_mono`] が担い、ここはその 1bpp
/// 結果に白/黒の色を当ててテクスチャへ載せるだけ (Web フロントと共有するため
/// ラスタライズ本体を core 側に置いた)。
struct Renderer {
    /// 使い回す 1bpp バッファ (IMG_W×IMG_H、0=消灯 1=点灯)。
    mono: Vec<u8>,
    /// 使い回す RGBA バッファ (IMG_W×IMG_H)。
    pixels: Vec<Color32>,
    texture: Option<TextureHandle>,
    /// 直前に描画した VRAM+PCG のスナップショット。
    last_vram_pcg: Vec<u8>,
    /// 直前に描画した状態。
    last_state: Option<RenderState>,
}

impl Renderer {
    fn new() -> Self {
        Self {
            mono: vec![0; IMG_W * IMG_H],
            pixels: vec![Color32::BLACK; IMG_W * IMG_H],
            texture: None,
            last_vram_pcg: Vec::new(),
            last_state: None,
        }
    }

    /// 表示状態が前回と変わっていればバッファを描き直し、テクスチャへ
    /// アップロードする。変化がなければ何もしない。
    fn sync(&mut self, ctx: &egui::Context, machine: &Machine, blink_phase: u32) {
        let state = RenderState::capture(machine, blink_phase);
        let vram = machine.vram();
        let pcg = machine.pcg();

        let unchanged = self.texture.is_some()
            && self.last_state.as_ref() == Some(&state)
            && self.last_vram_pcg.len() == vram.len() + pcg.len()
            && self.last_vram_pcg[..vram.len()] == *vram
            && self.last_vram_pcg[vram.len()..] == *pcg;
        if unchanged {
            return;
        }

        render_mono(&mut self.mono, machine, &state);
        for (dst, &on) in self.pixels.iter_mut().zip(self.mono.iter()) {
            *dst = if on != 0 { Color32::WHITE } else { Color32::BLACK };
        }

        // スナップショット更新 (確保は初回のみ、以降は容量を再利用)。
        self.last_vram_pcg.clear();
        self.last_vram_pcg.extend_from_slice(vram);
        self.last_vram_pcg.extend_from_slice(pcg);
        self.last_state = Some(state);

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

fn process_keyboard(ctx: &egui::Context, m: &mut Machine) {
    ctx.input(|i| {
        // F10: ローマ字 → 半角カナ変換のオン/オフ
        // (本家 IchigoJam の Ctrl+Space は macOS では OS の入力ソース
        // 切替に予約されているため、両 OS で動く F10 を採用)
        if i.key_pressed(Key::F10) {
            m.toggle_kana();
        }
        // 実機は keybuf を REPL 行編集と INKEY() で共有するが、本移植は
        // 行編集を input_putc/input_control が直接担うため keybuf は INKEY()
        // 専用。RUN 中以外は keybuf に積まない (REPL 編集中の文字を
        // INKEY() が拾ってしまうのを避けるため)。
        let executing = m.is_executing();

        for ev in &i.events {
            if let egui::Event::Key {
                key,
                physical_key,
                pressed: true,
                modifiers,
                ..
            } = ev
            {
                // ホスト側で別処理: Enter (REPL 行確定 / 実行中は keybuf)、
                // ESC (is_esc_pressed 立て)、F1-F12 (F キー割当)。
                // keymap は通さない。
                if matches!(*key, Key::Enter) {
                    if executing {
                        m.key_push(b'\n');
                    }
                    continue;
                }
                if is_host_reserved_key(*key) {
                    continue;
                }
                // OS のレイアウト変換は経由せず、物理キー位置で keymap を引く
                // (KBD コマンドの US/JA 切替を効かせるため)。physical_key が
                // 取れない環境では論理キーをフォールバックに使う。
                let phys = physical_key.unwrap_or(*key);
                let Some(hid) = egui_key_to_hid(phys, m.keyboard_id()) else {
                    continue;
                };
                let mut c = m.keymap_lookup(hid, modifiers.shift, modifiers.alt);
                if c == 0 {
                    continue;
                }
                // IchigoJam 慣習: 英字は常に大文字 (CAPS デフォルト ON)。
                // keymap の col0 (lowercase) を引いた場合のみ補正する。
                if c.is_ascii_lowercase() {
                    c -= b'a' - b'A';
                }
                if executing {
                    m.key_push(c);
                    continue;
                }
                if is_edit_control_code(c) {
                    // カナモード中の Backspace は未確定バッファ管理のため
                    // input_putc を通す。それ以外の編集キーは input_control。
                    if c == kc::BACKSPACE && m.is_kana_mode {
                        m.input_putc(c);
                    } else {
                        m.input_control(c);
                    }
                } else if c >= 128 {
                    // グラフィック文字 (128-255) はローマ字 → カナ変換を通さない
                    m.screen_putc(c);
                } else {
                    m.input_putc(c);
                }
            }
        }
        // ウィンドウがフォーカスを失っている間は解放イベントを取りこぼし、
        // BTN() のキーが押しっぱなしになるため、押下状態を一括クリアする。
        if !i.focused {
            m.key_clear_down();
        }
        for ev in &i.events {
            if let egui::Event::Key { key, pressed, .. } = ev {
                if let Some(code) = key_to_btn_code(*key) {
                    m.key_set_down(code, *pressed);
                }
            }
        }
    });
}

/// keymap の代わりにホスト側で別処理するキー。Enter / ESC / F キーは
/// IchigoApp の他の経路 (`execute_current_line`、`is_esc_pressed`、F キー
/// 割当) が拾うため、keymap に流すと二重入力になる。
fn is_host_reserved_key(k: Key) -> bool {
    matches!(
        k,
        Key::Escape
            | Key::F1
            | Key::F2
            | Key::F3
            | Key::F4
            | Key::F5
            | Key::F6
            | Key::F7
            | Key::F8
            | Key::F9
            | Key::F10
            | Key::F11
            | Key::F12
    )
}

/// keymap の戻り値のうち、REPL 編集を進める「制御コード」群。
/// これらは `input_control` 経由で画面エディタへ流す。
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

/// egui の物理キーを USB HID Keyboard Usage ID へ変換する。
/// 添字は HID Keyboard Usage ID に一致させる (例: 数字 2 キー = 0x1f、
/// `[` キー = 0x2f)。physical_key を引いて KBD コマンドの US/JA 切替を
/// OS レイアウトに依らず効かせるための入り口。
///
/// `Key::Backslash` だけ keyboard_id で Usage ID を出し分ける: winit は
/// US の `\` (0x31) と JIS の `]` (0x32) を同じ物理キーとして区別しない
/// ため (winit::KeyCode::Backslash の doc 参照)。`Key::Pipe` は含めない:
/// egui-winit は `KeyCode::IntlYen` (¥/| キー) を physical_key に変換
/// できず論理キーへフォールバックするため、layout に関わらず 0x31 固定
/// (0x31 の shift 列は US/JA 共通で `|`)。
fn egui_key_to_hid(k: Key, keyboard_id: u8) -> Option<u8> {
    Some(match k {
        // 英字: A=0x04 … Z=0x1d
        Key::A => 0x04,
        Key::B => 0x05,
        Key::C => 0x06,
        Key::D => 0x07,
        Key::E => 0x08,
        Key::F => 0x09,
        Key::G => 0x0a,
        Key::H => 0x0b,
        Key::I => 0x0c,
        Key::J => 0x0d,
        Key::K => 0x0e,
        Key::L => 0x0f,
        Key::M => 0x10,
        Key::N => 0x11,
        Key::O => 0x12,
        Key::P => 0x13,
        Key::Q => 0x14,
        Key::R => 0x15,
        Key::S => 0x16,
        Key::T => 0x17,
        Key::U => 0x18,
        Key::V => 0x19,
        Key::W => 0x1a,
        Key::X => 0x1b,
        Key::Y => 0x1c,
        Key::Z => 0x1d,
        // 数字行: 1=0x1e … 9=0x26、0=0x27
        Key::Num1 => 0x1e,
        Key::Num2 => 0x1f,
        Key::Num3 => 0x20,
        Key::Num4 => 0x21,
        Key::Num5 => 0x22,
        Key::Num6 => 0x23,
        Key::Num7 => 0x24,
        Key::Num8 => 0x25,
        Key::Num9 => 0x26,
        Key::Num0 => 0x27,
        // 制御 + Space
        Key::Backspace => 0x2a,
        Key::Tab => 0x2b,
        Key::Space => 0x2c,
        // 記号 (物理位置で引くため US 配列基準のキー名で対応する)
        Key::Minus => 0x2d,
        Key::Equals | Key::Plus => 0x2e,
        Key::OpenBracket => 0x2f,
        Key::CloseBracket => 0x30,
        Key::Backslash => {
            if keyboard_id == 0 {
                0x31
            } else {
                0x32
            }
        }
        Key::Pipe => 0x31,
        Key::Semicolon | Key::Colon => 0x33,
        Key::Quote => 0x34,
        Key::Backtick => 0x35,
        Key::Comma => 0x36,
        Key::Period => 0x37,
        Key::Slash | Key::Questionmark => 0x38,
        // カーソル / 編集系
        Key::Insert => 0x49,
        Key::Home => 0x4a,
        Key::PageUp => 0x4b,
        Key::Delete => 0x4c,
        Key::End => 0x4d,
        Key::PageDown => 0x4e,
        Key::ArrowRight => 0x4f,
        Key::ArrowLeft => 0x50,
        Key::ArrowDown => 0x51,
        Key::ArrowUp => 0x52,
        _ => return None,
    })
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

fn start_audio(tone: Arc<AtomicF32>) -> Result<cpal::Stream, String> {
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
                    let hz = tone.load(Ordering::Relaxed);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn egui_key_to_hid_covers_letters_and_digits() {
        assert_eq!(egui_key_to_hid(Key::A, 1), Some(0x04));
        assert_eq!(egui_key_to_hid(Key::Z, 1), Some(0x1d));
        assert_eq!(egui_key_to_hid(Key::Num1, 1), Some(0x1e));
        assert_eq!(egui_key_to_hid(Key::Num0, 1), Some(0x27));
    }

    #[test]
    fn egui_key_to_hid_covers_symbols() {
        // 物理キー位置で引けないと KBD 切替が効かないので必ず網羅する。
        assert_eq!(egui_key_to_hid(Key::Num2, 1), Some(0x1f));
        assert_eq!(egui_key_to_hid(Key::OpenBracket, 1), Some(0x2f));
        assert_eq!(egui_key_to_hid(Key::Quote, 1), Some(0x34));
        assert_eq!(egui_key_to_hid(Key::Semicolon, 1), Some(0x33));
    }

    #[test]
    fn egui_key_to_hid_backslash_depends_on_keyboard_id() {
        // US 101 キー配列の `\` = 0x31、JIS 106 キー配列の `]` = 0x32。
        // winit/egui は物理位置として両者を区別せず同じ Key::Backslash を
        // 報告するため、KBD で選んだ keyboard_id 側で Usage ID を出し分ける。
        assert_eq!(egui_key_to_hid(Key::Backslash, 0), Some(0x31));
        assert_eq!(egui_key_to_hid(Key::Backslash, 1), Some(0x32));
    }

    #[test]
    fn egui_key_to_hid_pipe_is_layout_independent() {
        // Key::Pipe は ¥/| キー (IntlYen) の論理キーフォールバックでのみ
        // 現れ、layout に関わらず 0x31 固定 (shift 列は US/JA 共通で `|`)。
        // Key::Backslash と同じ扱いにすると JIS 実機で Shift+¥/| が `}`
        // になってしまう (回帰)。
        assert_eq!(egui_key_to_hid(Key::Pipe, 0), Some(0x31));
        assert_eq!(egui_key_to_hid(Key::Pipe, 1), Some(0x31));
    }

    #[test]
    fn backslash_and_pipe_end_to_end_output_chars() {
        // egui_key_to_hid + keymap::lookup を通した最終出力文字まで確認する。
        // Backslash キー (JIS `]` キー) は layout で出力が変わるが、
        // Pipe キー (¥/| キー) は変わらないことを一度に検証する。
        let lookup = |key: Key, keyboard_id: u8, shift: bool| {
            let hid = egui_key_to_hid(key, keyboard_id).unwrap();
            ichigocrate_core::keymap::lookup(keyboard_id, hid, shift, false)
        };
        assert_eq!(lookup(Key::Backslash, 0, false), b'\\'); // US: \
        assert_eq!(lookup(Key::Backslash, 0, true), b'|'); // US+Shift: |
        assert_eq!(lookup(Key::Backslash, 1, false), b']'); // JA: ]
        assert_eq!(lookup(Key::Backslash, 1, true), b'}'); // JA+Shift: }
        assert_eq!(lookup(Key::Pipe, 0, true), b'|'); // US+Shift: |
        assert_eq!(lookup(Key::Pipe, 1, true), b'|'); // JA+Shift でも変わらず |
    }

    #[test]
    fn host_reserved_keys_skip_keymap() {
        // F キー / ESC は別経路。keymap を引かないようにマーク。
        assert!(is_host_reserved_key(Key::Escape));
        assert!(is_host_reserved_key(Key::F1));
        assert!(is_host_reserved_key(Key::F10));
        assert!(!is_host_reserved_key(Key::A));
        // Enter はホスト側でも処理するが is_host_reserved_key には含めず、
        // process_keyboard の上で個別に処理する。
        assert!(!is_host_reserved_key(Key::Enter));
    }
}
