//! ラスタライザ (`render_mono`) のビットマップ生成を検証する。
//! VRAM/PCG/フォントの 1bpp 展開、反転、VIDEO オフ、PCG 文字の経路を固定する。

use ichigocrate_core::font::CHAR_PATTERN_JP;
use ichigocrate_core::ram::OFFSET_RAM_PCG;
use ichigocrate_core::render::{render_mono, RenderState, FONT_H, FONT_W, IMG_H, IMG_W};
use ichigocrate_core::{exec_line, Machine};

/// VIDEO 表示 ON・反転/カーソルなしの素の描画状態。
fn plain() -> RenderState {
    RenderState {
        is_inverted: false,
        is_video_enabled: true,
        big: 0,
        cursor: None,
    }
}

/// セル (0,0) の文字を `c` にした Machine を作る。
fn machine_with_char(c: u8) -> Machine {
    let mut m = Machine::new();
    m.vram_mut()[0] = c;
    m
}

#[test]
fn renders_glyph_bits_at_top_left() {
    let ch = b'A';
    let m = machine_with_char(ch);
    let mut buf = vec![0u8; IMG_W * IMG_H];
    render_mono(&mut buf, &m, &plain());

    let glyph = &CHAR_PATTERN_JP[ch as usize * 8..ch as usize * 8 + 8];
    for row in 0..FONT_H {
        for col in 0..FONT_W {
            let expected = (glyph[row] >> (7 - col)) & 1;
            assert_eq!(buf[row * IMG_W + col], expected, "px ({col},{row})");
        }
    }
}

#[test]
fn invert_flips_every_pixel() {
    let m = machine_with_char(b'A');
    let mut normal = vec![0u8; IMG_W * IMG_H];
    let mut inverted = vec![0u8; IMG_W * IMG_H];
    render_mono(&mut normal, &m, &plain());
    render_mono(
        &mut inverted,
        &m,
        &RenderState {
            is_inverted: true,
            ..plain()
        },
    );
    for (n, i) in normal.iter().zip(inverted.iter()) {
        assert_eq!(*n ^ 1, *i);
    }
}

#[test]
fn video_off_blanks_buffer() {
    let m = machine_with_char(b'A');
    let mut buf = vec![1u8; IMG_W * IMG_H];
    render_mono(
        &mut buf,
        &m,
        &RenderState {
            is_video_enabled: false,
            ..plain()
        },
    );
    assert!(buf.iter().all(|&b| b == 0));
}

#[test]
fn cursor_full_width_inverts_whole_cell() {
    let m = machine_with_char(b' ');
    let mut buf = vec![0u8; IMG_W * IMG_H];
    render_mono(
        &mut buf,
        &m,
        &RenderState {
            cursor: Some((0, true)),
            ..plain()
        },
    );
    // 空白セル全体が反転 → 8x8 すべて点灯。
    for row in 0..FONT_H {
        for col in 0..FONT_W {
            assert_eq!(buf[row * IMG_W + col], 1, "px ({col},{row})");
        }
    }
}

#[test]
fn cursor_insert_inverts_left_half_only() {
    let m = machine_with_char(b' ');
    let mut buf = vec![0u8; IMG_W * IMG_H];
    render_mono(
        &mut buf,
        &m,
        &RenderState {
            cursor: Some((0, false)),
            ..plain()
        },
    );
    // 挿入モードカーソルは左半分 (col < 4) のみ反転。
    for row in 0..FONT_H {
        for col in 0..FONT_W {
            let expected = u8::from(col < FONT_W / 2);
            assert_eq!(buf[row * IMG_W + col], expected, "px ({col},{row})");
        }
    }
}

/// 0xE0-0xFF は font ではなく PCG (RAM) のパターンで描かれる。
#[test]
fn pcg_char_uses_ram_pattern() {
    let mut m = machine_with_char(0xE0);
    // 0xE0 の 8 行すべてを左半分点灯 (0xF0) に書き換える
    m.ram[OFFSET_RAM_PCG..OFFSET_RAM_PCG + FONT_H].fill(0xf0);
    let mut buf = vec![0u8; IMG_W * IMG_H];
    render_mono(&mut buf, &m, &plain());
    for row in 0..FONT_H {
        for col in 0..FONT_W {
            let expected = u8::from(col < FONT_W / 2);
            assert_eq!(buf[row * IMG_W + col], expected, "px ({col},{row})");
        }
    }
}

/// VIDEO 3 (big=1) では 1 ソースピクセルが 2x2 ブロックに拡大される。
#[test]
fn big_mode_scales_each_pixel_to_block() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "VIDEO 3");
    m.vram_mut()[0] = b'A';
    let state = RenderState::capture(&m, 1);
    assert_eq!(state.big, 1);
    let mut buf = vec![0u8; IMG_W * IMG_H];
    render_mono(&mut buf, &m, &state);

    let glyph = &CHAR_PATTERN_JP[b'A' as usize * 8..b'A' as usize * 8 + 8];
    for y in 0..FONT_H * 2 {
        for x in 0..FONT_W * 2 {
            let expected = (glyph[y / 2] >> (7 - x / 2)) & 1;
            assert_eq!(buf[y * IMG_W + x], expected, "px ({x},{y})");
        }
    }
}

/// capture: カーソルは表示中かつ点滅位相が偶数のときだけ、セル index で返る。
#[test]
fn capture_reports_cursor_only_on_visible_phase() {
    let mut m = Machine::new();
    m.is_cursor_visible = true;
    m.screen_locate(3, 2);
    let cursor = RenderState::capture(&m, 0).cursor;
    assert_eq!(cursor.map(|(i, _)| i), Some(2 * 32 + 3));
    assert_eq!(RenderState::capture(&m, 1).cursor, None);

    m.is_cursor_visible = false;
    assert_eq!(RenderState::capture(&m, 0).cursor, None);
}
