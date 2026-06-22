// Copyright 2014-2024 the IchigoJam authors. All rights reserved. MIT license.
// https://github.com/IchigoJam/ichigojam-firm/blob/main/IchigoJam_P/src/hid.h

//! HID キーコード → ASCII 変換テーブル (US/JA 両配列)。
//!
//! 元 C 実装 `HID_KEYCODE_TO_ASCII_US` / `HID_KEYCODE_TO_ASCII_JA` を
//! `[[u8; 4]; 128]` (列: plain / shift / alt / alt+shift) でそのまま保持する。
//! ホストは `Machine::keymap_lookup` 経由で `keyboard_id` に応じた表を引く。

use crate::keycodes as kc;

// 元 C コードに合わせた制御コード別名 (本表内のみで使用)。
const RETURN: u8 = b'\n';
const SFTSP: u8 = 14;
const SFTRET: u8 = 16;
const INSERT: u8 = kc::INSERT_TOGGLE;
const HOME: u8 = kc::HOME;
const PGUP: u8 = kc::PAGE_UP;
const END: u8 = kc::END;
const PGDOWN: u8 = kc::PAGE_DOWN;
const ESC: u8 = 27;
const LEFT: u8 = kc::CURSOR_LEFT;
const RIGHT: u8 = kc::CURSOR_RIGHT;
const UP: u8 = kc::CURSOR_UP;
const DOWN: u8 = kc::CURSOR_DOWN;
const DELETE: u8 = kc::DELETE;
const BS: u8 = kc::BACKSPACE;
const TAB: u8 = kc::TAB;

const fn empty_table() -> [[u8; 4]; 128] {
    [[0u8; 4]; 128]
}

const fn build_us() -> [[u8; 4]; 128] {
    let mut t = empty_table();
    // 0x04-0x1d: a-z (グラフィック文字割当は a→234..v→255, w→224..z→227,
    // alt+shift は -96 で 128-159)
    t[0x04] = [b'a', b'A', 234, 138];
    t[0x05] = [b'b', b'B', 235, 139];
    t[0x06] = [b'c', b'C', 236, 140];
    t[0x07] = [b'd', b'D', 237, 141];
    t[0x08] = [b'e', b'E', 238, 142];
    t[0x09] = [b'f', b'F', 239, 143];
    t[0x0a] = [b'g', b'G', 240, 144];
    t[0x0b] = [b'h', b'H', 241, 145];
    t[0x0c] = [b'i', b'I', 242, 146];
    t[0x0d] = [b'j', b'J', 243, 147];
    t[0x0e] = [b'k', b'K', 244, 148];
    t[0x0f] = [b'l', b'L', 245, 149];
    t[0x10] = [b'm', b'M', 246, 150];
    t[0x11] = [b'n', b'N', 247, 151];
    t[0x12] = [b'o', b'O', 248, 152];
    t[0x13] = [b'p', b'P', 249, 153];
    t[0x14] = [b'q', b'Q', 250, 154];
    t[0x15] = [b'r', b'R', 251, 155];
    t[0x16] = [b's', b'S', 252, 156];
    t[0x17] = [b't', b'T', 253, 157];
    t[0x18] = [b'u', b'U', 254, 158];
    t[0x19] = [b'v', b'V', 255, 159];
    t[0x1a] = [b'w', b'W', 224, 128];
    t[0x1b] = [b'x', b'X', 225, 129];
    t[0x1c] = [b'y', b'Y', 226, 130];
    t[0x1d] = [b'z', b'Z', 227, 131];
    // 0x1e-0x27: 1234567890
    t[0x1e] = [b'1', b'!', 225, 129];
    t[0x1f] = [b'2', b'@', 226, 130];
    t[0x20] = [b'3', b'#', 227, 131];
    t[0x21] = [b'4', b'$', 228, 132];
    t[0x22] = [b'5', b'%', 229, 133];
    t[0x23] = [b'6', b'^', 230, 134];
    t[0x24] = [b'7', b'&', 231, 135];
    t[0x25] = [b'8', b'*', 232, 136];
    t[0x26] = [b'9', b'(', 233, 137];
    t[0x27] = [b'0', b')', 224, 128];
    // 0x28-0x2c: 制御 + Space
    t[0x28] = [RETURN, SFTRET, RETURN, SFTRET];
    t[0x29] = [ESC, ESC, ESC, ESC];
    t[0x2a] = [BS, BS, BS, BS];
    t[0x2b] = [TAB, TAB, TAB, TAB];
    t[0x2c] = [b' ', SFTSP, b' ', SFTSP];
    // 0x2d-0x38: 記号 (US 配列)
    t[0x2d] = [b'-', b'_', b'-', b'_'];
    t[0x2e] = [b'=', b'+', b'=', b'+'];
    t[0x2f] = [b'[', b'{', b'_', b'_'];
    t[0x30] = [b']', b'}', b'\\', b'\\'];
    t[0x31] = [b'\\', b'|', b'\\', b'|'];
    t[0x32] = [b'#', b'~', b'#', b'~'];
    t[0x33] = [b';', b':', b';', b':'];
    t[0x34] = [b'\'', b'"', b'\'', b'"'];
    t[0x35] = [b'`', b'~', b'`', b'~'];
    t[0x36] = [b',', b'<', b',', b'<'];
    t[0x37] = [b'.', b'>', b'.', b'>'];
    t[0x38] = [b'/', b'?', b'/', b'?'];
    // 0x39: CapsLock (出力なし), 0x3a-0x45: F1-F12 (本移植では未割当)
    // 0x49-0x52: 編集/カーソル系
    t[0x49] = [INSERT, INSERT, INSERT, INSERT];
    t[0x4a] = [HOME, HOME, HOME, HOME];
    t[0x4b] = [PGUP, PGUP, PGUP, PGUP];
    t[0x4c] = [DELETE, DELETE, DELETE, DELETE];
    t[0x4d] = [END, END, END, END];
    t[0x4e] = [PGDOWN, PGDOWN, PGDOWN, PGDOWN];
    t[0x4f] = [RIGHT, RIGHT, RIGHT, RIGHT];
    t[0x50] = [LEFT, LEFT, LEFT, LEFT];
    t[0x51] = [DOWN, DOWN, DOWN, DOWN];
    t[0x52] = [UP, UP, UP, UP];
    // 0x54-0x67: テンキー (NumLock 想定なし、即出力)
    t[0x54] = [b'/', b'/', b'/', b'/'];
    t[0x55] = [b'*', b'*', b'*', b'*'];
    t[0x56] = [b'-', b'-', b'-', b'-'];
    t[0x57] = [b'+', b'+', b'+', b'+'];
    t[0x58] = [RETURN, SFTRET, RETURN, SFTRET];
    t[0x59] = [b'1', b'1', 225, 129];
    t[0x5a] = [b'2', b'2', 226, 130];
    t[0x5b] = [b'3', b'3', 227, 131];
    t[0x5c] = [b'4', b'4', 228, 132];
    t[0x5d] = [b'5', b'5', 229, 133];
    t[0x5e] = [b'6', b'6', 230, 134];
    t[0x5f] = [b'7', b'7', 231, 135];
    t[0x60] = [b'8', b'8', 232, 136];
    t[0x61] = [b'9', b'9', 233, 137];
    t[0x62] = [b'0', b'0', 224, 128];
    t[0x63] = [b'.', b'.', b'.', b'.'];
    t[0x67] = [b'=', b'=', b'=', b'='];
    t
}

const fn build_ja() -> [[u8; 4]; 128] {
    let mut t = empty_table();
    // 英字行は US と同一
    t[0x04] = [b'a', b'A', 234, 138];
    t[0x05] = [b'b', b'B', 235, 139];
    t[0x06] = [b'c', b'C', 236, 140];
    t[0x07] = [b'd', b'D', 237, 141];
    t[0x08] = [b'e', b'E', 238, 142];
    t[0x09] = [b'f', b'F', 239, 143];
    t[0x0a] = [b'g', b'G', 240, 144];
    t[0x0b] = [b'h', b'H', 241, 145];
    t[0x0c] = [b'i', b'I', 242, 146];
    t[0x0d] = [b'j', b'J', 243, 147];
    t[0x0e] = [b'k', b'K', 244, 148];
    t[0x0f] = [b'l', b'L', 245, 149];
    t[0x10] = [b'm', b'M', 246, 150];
    t[0x11] = [b'n', b'N', 247, 151];
    t[0x12] = [b'o', b'O', 248, 152];
    t[0x13] = [b'p', b'P', 249, 153];
    t[0x14] = [b'q', b'Q', 250, 154];
    t[0x15] = [b'r', b'R', 251, 155];
    t[0x16] = [b's', b'S', 252, 156];
    t[0x17] = [b't', b'T', 253, 157];
    t[0x18] = [b'u', b'U', 254, 158];
    t[0x19] = [b'v', b'V', 255, 159];
    t[0x1a] = [b'w', b'W', 224, 128];
    t[0x1b] = [b'x', b'X', 225, 129];
    t[0x1c] = [b'y', b'Y', 226, 130];
    t[0x1d] = [b'z', b'Z', 227, 131];
    // 数字行: Shift 列が US と異なる (例: Shift+2 が " になる)
    t[0x1e] = [b'1', b'!', 225, 129];
    t[0x1f] = [b'2', b'"', 226, 130];
    t[0x20] = [b'3', b'#', 227, 131];
    t[0x21] = [b'4', b'$', 228, 132];
    t[0x22] = [b'5', b'%', 229, 133];
    t[0x23] = [b'6', b'&', 230, 134];
    t[0x24] = [b'7', b'\'', 231, 135];
    t[0x25] = [b'8', b'(', 232, 136];
    t[0x26] = [b'9', b')', 233, 137];
    t[0x27] = [b'0', b'0', 224, 128];
    // 0x28-0x2c: 制御 + Space (US と同一)
    t[0x28] = [RETURN, SFTRET, RETURN, SFTRET];
    t[0x29] = [ESC, ESC, ESC, ESC];
    t[0x2a] = [BS, BS, BS, BS];
    t[0x2b] = [TAB, TAB, TAB, TAB];
    t[0x2c] = [b' ', SFTSP, b' ', SFTSP];
    // 0x2d-0x38: JA 配列特有の記号配置
    t[0x2d] = [b'-', b'=', b'-', b'='];
    t[0x2e] = [b'^', b'~', b'^', b'~'];
    t[0x2f] = [b'@', b'`', b'@', b'`'];
    t[0x30] = [b'[', b'{', b'_', b'_'];
    t[0x31] = [b'\\', b'|', b'\\', b'|'];
    t[0x32] = [b']', b'}', b'\\', b'\\'];
    t[0x33] = [b';', b'+', b';', b'+'];
    t[0x34] = [b':', b'*', b':', b'*'];
    // 0x35: 全角/半角キー、本家に合わせてスペース出力
    t[0x35] = [b' ', b' ', b' ', b' '];
    t[0x36] = [b',', b'<', b',', b'<'];
    t[0x37] = [b'.', b'>', b'.', b'>'];
    t[0x38] = [b'/', b'?', b'/', b'?'];
    // 0x49-0x52: 編集/カーソル系 (US と同一)
    t[0x49] = [INSERT, INSERT, INSERT, INSERT];
    t[0x4a] = [HOME, HOME, HOME, HOME];
    t[0x4b] = [PGUP, PGUP, PGUP, PGUP];
    t[0x4c] = [DELETE, DELETE, DELETE, DELETE];
    t[0x4d] = [END, END, END, END];
    t[0x4e] = [PGDOWN, PGDOWN, PGDOWN, PGDOWN];
    t[0x4f] = [RIGHT, RIGHT, RIGHT, RIGHT];
    t[0x50] = [LEFT, LEFT, LEFT, LEFT];
    t[0x51] = [DOWN, DOWN, DOWN, DOWN];
    t[0x52] = [UP, UP, UP, UP];
    // テンキー (US と同一)
    t[0x54] = [b'/', b'/', b'/', b'/'];
    t[0x55] = [b'*', b'*', b'*', b'*'];
    t[0x56] = [b'-', b'-', b'-', b'-'];
    t[0x57] = [b'+', b'+', b'+', b'+'];
    t[0x58] = [RETURN, SFTRET, RETURN, SFTRET];
    t[0x59] = [b'1', b'1', 225, 129];
    t[0x5a] = [b'2', b'2', 226, 130];
    t[0x5b] = [b'3', b'3', 227, 131];
    t[0x5c] = [b'4', b'4', 228, 132];
    t[0x5d] = [b'5', b'5', 229, 133];
    t[0x5e] = [b'6', b'6', 230, 134];
    t[0x5f] = [b'7', b'7', 231, 135];
    t[0x60] = [b'8', b'8', 232, 136];
    t[0x61] = [b'9', b'9', 233, 137];
    t[0x62] = [b'0', b'0', 224, 128];
    t[0x63] = [b'.', b'.', b'.', b'.'];
    t[0x67] = [b'=', b'=', b'=', b'='];
    t
}

pub const KEYMAP_US: [[u8; 4]; 128] = build_us();
pub const KEYMAP_JA: [[u8; 4]; 128] = build_ja();

/// HID キーコード + 修飾キー → IchigoJam 内部コード。
/// `keyboard_id` 0 = US、それ以外 = JA (`KBD` コマンドの `!!n` 正規化と一致)。
/// 0 はそのキーに対する出力が割り当てられていないことを表す。
pub fn lookup(keyboard_id: u8, hid: u8, shift: bool, alt: bool) -> u8 {
    if hid as usize >= 128 {
        return 0;
    }
    let table = if keyboard_id == 0 { &KEYMAP_US } else { &KEYMAP_JA };
    let col = (alt as usize) * 2 + (shift as usize);
    table[hid as usize][col]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_2_differs_between_layouts() {
        // ユーザー報告例: 日本語配列で Shift+2 を打つと、KBD 0 (US) では @
        // が出るべき。JA では " が出る。
        assert_eq!(lookup(0, 0x1f, true, false), b'@');
        assert_eq!(lookup(1, 0x1f, true, false), b'"');
    }

    #[test]
    fn alpha_uppercase_via_shift_col() {
        // 英字行はどちらの配列でも一致 (大小は shift 列で切り替わる)
        assert_eq!(lookup(0, 0x04, false, false), b'a');
        assert_eq!(lookup(0, 0x04, true, false), b'A');
        assert_eq!(lookup(1, 0x04, false, false), b'a');
        assert_eq!(lookup(1, 0x04, true, false), b'A');
    }

    #[test]
    fn alt_letter_yields_graphic_char() {
        // Alt+a → 234 (US/JA 共通)、Alt+Shift+a → 138
        assert_eq!(lookup(0, 0x04, false, true), 234);
        assert_eq!(lookup(0, 0x04, true, true), 138);
        assert_eq!(lookup(1, 0x04, false, true), 234);
    }

    #[test]
    fn symbol_row_differs_between_layouts() {
        // 0x2f: US `[` / Shift `{`、JA `@` / Shift ``
        assert_eq!(lookup(0, 0x2f, false, false), b'[');
        assert_eq!(lookup(1, 0x2f, false, false), b'@');
        assert_eq!(lookup(0, 0x2f, true, false), b'{');
        assert_eq!(lookup(1, 0x2f, true, false), b'`');
        // 0x34: US `'` / `"`、JA `:` / `*`
        assert_eq!(lookup(0, 0x34, true, false), b'"');
        assert_eq!(lookup(1, 0x34, true, false), b'*');
    }

    #[test]
    fn out_of_range_hid_returns_zero() {
        assert_eq!(lookup(0, 0x7f, false, false), 0);
        assert_eq!(lookup(0, 0xff, true, true), 0);
    }

    #[test]
    fn control_keys_table_driven() {
        // 矢印 / Backspace / Enter なども同じ表から取れる
        assert_eq!(lookup(0, 0x4f, false, false), kc::CURSOR_RIGHT);
        assert_eq!(lookup(0, 0x52, false, false), kc::CURSOR_UP);
        assert_eq!(lookup(0, 0x2a, false, false), kc::BACKSPACE);
        assert_eq!(lookup(0, 0x28, false, false), b'\n');
    }
}
