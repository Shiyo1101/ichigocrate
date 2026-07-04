//! VRAM/PCG/フォントを 1bpp のモノクロ画面ビットマップへ展開する純粋ラスタライザ。
//!
//! 前景/背景色の割当てや LED 枠の描画といった色の決定はフロントエンド (ネイティブ
//! egui / Web canvas) 側の責務とし、ここでは egui などの描画フレームワークに依存
//! しない「点灯/消灯」のビットマップだけを生成する。これによりネイティブと Web で
//! 同一のラスタライズ結果を共有できる。

use crate::font::CHAR_PATTERN_JP;
use crate::machine::Machine;
use crate::ram::{SCREEN_H, SCREEN_W};

/// フォント 1 文字のピクセル幅。
pub const FONT_W: usize = 8;
/// フォント 1 文字のピクセル高さ。
pub const FONT_H: usize = 8;
/// 画面ビットマップの幅 (VIDEO 拡大時も同一サイズへ引き伸ばす)。
pub const IMG_W: usize = SCREEN_W * FONT_W;
/// 画面ビットマップの高さ。
pub const IMG_H: usize = SCREEN_H * FONT_H;

/// 描画結果に影響する VRAM/PCG 以外の状態。フロント側の dirty 判定にも使う。
#[derive(Clone, PartialEq, Eq)]
pub struct RenderState {
    pub is_inverted: bool,
    pub is_video_enabled: bool,
    /// 拡大段階 (表示倍率は `1 << big`)。
    pub big: u8,
    /// 反転描画されるカーソル: (セル index, 全角なら true)。非表示時は `None`。
    pub cursor: Option<(usize, bool)>,
}

impl RenderState {
    /// 現在の [`Machine`] 状態とカーソル点滅位相から描画状態を取り出す。
    ///
    /// `blink_phase` の bit0 が 0 のフレームでだけカーソルを点灯させる
    /// (点滅はフロント側が位相を進めて与える)。
    pub fn capture(m: &Machine, blink_phase: u32) -> Self {
        let cols = m.screen_cols();
        let rows = m.screen_rows();
        let show = m.is_cursor_visible && (blink_phase & 1) == 0;
        let in_range = m.cursory >= 0
            && (m.cursory as usize) < rows
            && m.cursorx >= 0
            && (m.cursorx as usize) < cols;
        let cursor = if show && in_range {
            // 上書きモードは文字全体、挿入モードは左半分のみ反転 (実機準拠)。
            Some((m.cursory as usize * cols + m.cursorx as usize, m.cursor_full_width()))
        } else {
            None
        };
        Self {
            is_inverted: m.is_screen_inverted,
            is_video_enabled: m.is_video_enabled,
            big: m.screen_big.min(3),
            cursor,
        }
    }
}

/// `state` に従い VRAM を 1bpp ビットマップ `buf` へ描く。
///
/// `buf` の長さは [`IMG_W`] × [`IMG_H`] であること。各バイトは 0 (背景=消灯) か
/// 1 (前景=点灯) で、色付けは呼び出し側で行う。
///
/// VIDEO 3/4 の拡大表示では論理画面サイズ (cols×rows) が `SCREEN_W/H >> big` に
/// 縮み、VRAM のストライドも cols になる。倍率 `zoom = 1 << big` を掛けると
/// `cols*zoom*FONT_W == IMG_W` となるため、可視領域をそのまま IMG_W×IMG_H へ
/// 引き伸ばせる。
pub fn render_mono(buf: &mut [u8], machine: &Machine, state: &RenderState) {
    buf.fill(0);
    // VIDEO 0: 映像オフ。VRAM の内容に関わらず消灯画面。
    if !state.is_video_enabled {
        return;
    }

    let vram = machine.vram();
    let pcg = machine.pcg();
    let zoom = 1usize << state.big as u32;
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
                &CHAR_PATTERN_JP[p..p + 8]
            };
            let cursor_here = matches!(state.cursor, Some((i, _)) if i == idx);
            let cursor_full = matches!(state.cursor, Some((i, full)) if i == idx && full);
            for (row, &bits) in glyph.iter().enumerate() {
                for col in 0..FONT_W {
                    let bit = (bits >> (7 - col)) & 1 != 0;
                    let mut on = bit;
                    if state.is_inverted {
                        on = !on;
                    }
                    // カーソル反転。挿入モードは左半分 (col < 4) のみ反転して細いカーソルにする。
                    if cursor_here && (cursor_full || col < FONT_W / 2) {
                        on = !on;
                    }
                    // 1 ソースピクセルを zoom×zoom ブロックに展開
                    let px0 = (cx * FONT_W + col) * zoom;
                    let py0 = (cy * FONT_H + row) * zoom;
                    for dy in 0..zoom {
                        for dx in 0..zoom {
                            buf[(py0 + dy) * IMG_W + (px0 + dx)] = on as u8;
                        }
                    }
                }
            }
        }
    }
}
