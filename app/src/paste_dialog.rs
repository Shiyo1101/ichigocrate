//! プログラムの貼り付け (Cmd+V / Ctrl+V) の確認モーダル。
//!
//! 貼り付けたテキストをいきなり打ち込まず、行数・中身・注意点を見せてから流し込む。
//! 見た目は IchigoJam の画面に合わせ、黒地に白の枠・等幅フォント・英大文字にする
//! (アプリに日本語フォントは入れていない)。プレビューは IchigoJam のフォントで、
//! 画面と同じ 32 文字幅で折り返して描くので、カナやグラフィック文字もそのまま見える。

use eframe::egui;
use egui::{Align2, Color32, ColorImage, FontId, Key, RichText, Rounding, Stroke, TextureHandle};

use ichigocrate_core::font::CHAR_PATTERN_JP;
use ichigocrate_core::paste::{PreparedPaste, MAX_LINE_CHARS};
use ichigocrate_core::SCREEN_W;

/// モーダルの幅 (ウィンドウは IchigoJam の画面 800px 程度に合わせてある)。
/// プレビュー (32 文字 × 8px × 2 倍 = 512px) が収まる幅にする。
const DIALOG_WIDTH: f32 = 600.0;
/// IchigoJam の文字の大きさ (px)。
const GLYPH_SIZE: usize = 8;
/// プレビューの拡大率 (整数倍にしてドットをくっきり見せる)。
const PREVIEW_SCALE: f32 = 2.0;
/// プレビュー欄の最大の高さ。これを超える分はスクロールする。
const PREVIEW_MAX_HEIGHT: f32 = 220.0;
/// 「その場で実行される行」として例示する最大の数。
const MAX_LISTED_IMMEDIATE_LINES: usize = 3;
/// 注意の文字色。
const WARNING_COLOR: Color32 = Color32::from_rgb(255, 210, 80);
/// 補足の文字色。
const MUTED_COLOR: Color32 = Color32::from_gray(160);
/// 背景を暗くする幕の色。
const BACKDROP_COLOR: Color32 = Color32::from_black_alpha(190);

/// 貼り付け前の確認の状態。
pub struct PasteDialog {
    prepared: PreparedPaste,
    /// 貼り付け前に NEW して、今のプログラムを消すか。
    new_first: bool,
    /// プレビュー画像 (初めて描くときに作る)。
    preview: Option<TextureHandle>,
}

/// モーダルでの操作の結果。
pub enum PasteAction {
    /// 貼り付ける。流し込むバイト列 (必要なら先頭に `NEW`)。
    Confirm(Vec<u8>),
    Cancel,
}

impl PasteDialog {
    pub fn new(prepared: PreparedPaste) -> Self {
        // プログラム (行番号付きの行) を貼るときは、今のプログラムと混ざらないよう既定で NEW する
        let new_first = prepared.numbered_lines > 0;
        Self {
            prepared,
            new_first,
            preview: None,
        }
    }

    /// モーダルを描き、操作があれば返す。`is_running` はプログラムの実行中か
    /// (貼り付けると実行中のプログラムを止める)。Enter で貼り付け、Esc で取り消し。
    pub fn show(&mut self, ctx: &egui::Context, is_running: bool) -> Option<PasteAction> {
        show_backdrop(ctx);

        let mut action = None;
        let can_paste = !self.prepared.is_empty();
        egui::Window::new("paste_program")
            .title_bar(false)
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .min_width(DIALOG_WIDTH)
            .max_width(DIALOG_WIDTH)
            .resizable(false)
            .collapsible(false)
            .movable(false)
            .frame(
                egui::Frame::none()
                    .fill(Color32::BLACK)
                    .stroke(Stroke::new(2.0, Color32::WHITE))
                    .rounding(Rounding::ZERO)
                    .inner_margin(20.0),
            )
            .show(ctx, |ui| {
                apply_retro_style(ui);
                ui.label(mono("PASTE PROGRAM", 20.0).color(Color32::WHITE).strong());
                ui.add_space(4.0);
                if can_paste {
                    ui.label(mono(&self.summary(), 13.0).color(MUTED_COLOR));
                    ui.add_space(10.0);
                    self.show_preview(ui, ctx);
                    self.show_warnings(ui, is_running);
                    ui.add_space(10.0);
                    ui.checkbox(
                        &mut self.new_first,
                        mono("NEW FIRST (CLEAR CURRENT PROGRAM)", 13.0),
                    );
                } else {
                    ui.add_space(10.0);
                    ui.label(mono("NOTHING TO PASTE.", 14.0).color(WARNING_COLOR));
                }
                ui.add_space(16.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if can_paste {
                        let label = if is_running {
                            "STOP & PASTE  [ENTER]"
                        } else {
                            "PASTE  [ENTER]"
                        };
                        let primary =
                            egui::Button::new(mono(label, 14.0).color(Color32::BLACK).strong())
                                .fill(Color32::WHITE);
                        if ui.add(primary).clicked() {
                            action = Some(PasteAction::Confirm(self.bytes_to_feed()));
                        }
                        ui.add_space(8.0);
                    }
                    let cancel = if can_paste {
                        "CANCEL  [ESC]"
                    } else {
                        "CLOSE  [ESC]"
                    };
                    let secondary = egui::Button::new(mono(cancel, 14.0));
                    if ui.add(secondary).clicked() {
                        action = Some(PasteAction::Cancel);
                    }
                });
            });

        // キーボードでも操作できるようにする (ボタンより後に判定しても同じフレームで効く)
        if action.is_none() {
            let (enter, escape) =
                ctx.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
            if escape {
                action = Some(PasteAction::Cancel);
            } else if enter && can_paste {
                action = Some(PasteAction::Confirm(self.bytes_to_feed()));
            }
        }
        action
    }

    fn summary(&self) -> String {
        let p = &self.prepared;
        let lines = plural(p.lines, "LINE");
        let bytes = plural(p.bytes.len(), "BYTE");
        format!("{lines}  /  {bytes}")
    }

    fn show_preview(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let prepared = &self.prepared;
        let texture = self.preview.get_or_insert_with(|| {
            ctx.load_texture(
                "paste_preview",
                render_preview(&prepared.bytes),
                egui::TextureOptions::NEAREST,
            )
        });
        let size = texture.size_vec2() * PREVIEW_SCALE;
        egui::Frame::none()
            .stroke(Stroke::new(1.0, Color32::from_gray(90)))
            .inner_margin(8.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(PREVIEW_MAX_HEIGHT)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.image((texture.id(), size));
                    });
            });
    }

    fn show_warnings(&self, ui: &mut egui::Ui, is_running: bool) {
        let p = &self.prepared;
        let mut warnings = Vec::new();
        if !p.immediate_lines.is_empty() {
            let mut listed: Vec<&str> = p
                .immediate_lines
                .iter()
                .take(MAX_LISTED_IMMEDIATE_LINES)
                .map(String::as_str)
                .collect();
            if p.immediate_lines.len() > MAX_LISTED_IMMEDIATE_LINES {
                listed.push("...");
            }
            warnings.push(format!(
                "{} WITHOUT LINE NUMBER WILL RUN NOW: {}",
                plural(p.immediate_lines.len(), "LINE"),
                listed.join(", ")
            ));
        }
        if p.long_lines > 0 {
            warnings.push(format!(
                "{} LONGER THAN {MAX_LINE_CHARS} CHARS WILL BE CUT",
                plural(p.long_lines, "LINE")
            ));
        }
        if p.skipped_chars > 0 {
            warnings.push(format!(
                "{} NOT AVAILABLE ON ICHIGOJAM WILL BE SKIPPED",
                plural(p.skipped_chars, "CHAR")
            ));
        }
        if is_running {
            warnings.push("THE RUNNING PROGRAM WILL BE STOPPED".to_string());
        }
        if warnings.is_empty() {
            return;
        }
        ui.add_space(8.0);
        for warning in warnings {
            ui.label(mono(&format!("! {warning}"), 12.0).color(WARNING_COLOR));
        }
    }

    fn bytes_to_feed(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.prepared.bytes.len() + 4);
        if self.new_first {
            bytes.extend_from_slice(b"NEW\n");
        }
        bytes.extend_from_slice(&self.prepared.bytes);
        bytes
    }
}

/// モーダルの後ろを暗くし、後ろの画面への操作を受け付けないようにする。
fn show_backdrop(ctx: &egui::Context) {
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("paste_backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, Rounding::ZERO, BACKDROP_COLOR);
        });
}

/// ボタンやチェックボックスを、黒地・白枠・角なしにする。
fn apply_retro_style(ui: &mut egui::Ui) {
    let visuals = &mut ui.style_mut().visuals;
    for (widget, fill, stroke) in [
        (
            &mut visuals.widgets.inactive,
            Color32::BLACK,
            Color32::from_gray(200),
        ),
        (
            &mut visuals.widgets.hovered,
            Color32::from_gray(40),
            Color32::WHITE,
        ),
        (
            &mut visuals.widgets.active,
            Color32::from_gray(70),
            Color32::WHITE,
        ),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::new(1.0, stroke);
        widget.fg_stroke = Stroke::new(1.5, Color32::WHITE);
        widget.rounding = Rounding::ZERO;
    }
    // ボタンは内側の余白で大きくする (高さを指定すると文字が上に寄るため)
    ui.style_mut().spacing.button_padding = egui::vec2(14.0, 8.0);
    let visuals = &mut ui.style_mut().visuals;
    visuals.selection.bg_fill = Color32::WHITE;
    visuals.selection.stroke = Stroke::new(1.0, Color32::BLACK);
}

fn mono(text: &str, size: f32) -> RichText {
    RichText::new(text).font(FontId::monospace(size))
}

/// `1 LINE` / `2 LINES` のように数と単位を並べる。
fn plural(n: usize, unit: &str) -> String {
    if n == 1 {
        format!("1 {unit}")
    } else {
        format!("{n} {unit}S")
    }
}

/// 流し込むバイト列を、画面と同じ 32 文字幅で折り返した行に分ける。
fn wrap_rows(bytes: &[u8]) -> Vec<&[u8]> {
    bytes
        .split(|&b| b == b'\n')
        .filter(|line| !line.is_empty())
        .flat_map(|line| line.chunks(SCREEN_W))
        .collect()
}

/// 流し込むバイト列を IchigoJam のフォントで描いた画像 (白黒、1 文字 8×8 ドット)。
fn render_preview(bytes: &[u8]) -> ColorImage {
    let rows = wrap_rows(bytes);
    let width = SCREEN_W * GLYPH_SIZE;
    let height = rows.len().max(1) * GLYPH_SIZE;
    let mut pixels = vec![Color32::BLACK; width * height];
    for (row, chars) in rows.iter().enumerate() {
        for (col, &c) in chars.iter().enumerate() {
            let glyph = &CHAR_PATTERN_JP[usize::from(c) * GLYPH_SIZE..][..GLYPH_SIZE];
            for (y, bits) in glyph.iter().enumerate() {
                for x in 0..GLYPH_SIZE {
                    if bits & (0x80 >> x) != 0 {
                        let px = col * GLYPH_SIZE + x;
                        let py = row * GLYPH_SIZE + y;
                        pixels[py * width + px] = Color32::WHITE;
                    }
                }
            }
        }
    }
    ColorImage {
        size: [width, height],
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ichigocrate_core::paste::prepare_paste;

    #[test]
    fn new_first_defaults_on_only_for_programs() {
        assert!(PasteDialog::new(prepare_paste("10 ?1\n")).new_first);
        assert!(!PasteDialog::new(prepare_paste("?1+1\n")).new_first);
    }

    #[test]
    fn feeds_new_before_program_when_checked() {
        let mut dialog = PasteDialog::new(prepare_paste("10 ?1"));
        assert_eq!(dialog.bytes_to_feed(), b"NEW\n10 ?1\n");
        dialog.new_first = false;
        assert_eq!(dialog.bytes_to_feed(), b"10 ?1\n");
    }

    #[test]
    fn preview_wraps_at_screen_width() {
        let long = "1".repeat(SCREEN_W + 1);
        let prepared = prepare_paste(&format!("10 ?\"ｱ\"\n{long}"));
        let rows = wrap_rows(&prepared.bytes);
        assert_eq!(rows.len(), 3);
        // カナは IchigoJam の文字コードのまま描く
        assert_eq!(rows[0], [b"10 ?\"".as_slice(), &[0xB1], b"\""].concat());
        assert_eq!(rows[2], b"1");
        let image = render_preview(&prepared.bytes);
        assert_eq!(image.size, [SCREEN_W * GLYPH_SIZE, 3 * GLYPH_SIZE]);
        assert!(image.pixels.contains(&Color32::WHITE));
    }

    #[test]
    fn summary_uses_singular_and_plural() {
        assert_eq!(
            PasteDialog::new(prepare_paste("10 ?1")).summary(),
            "1 LINE  /  6 BYTES"
        );
    }
}
