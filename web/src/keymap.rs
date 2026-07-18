//! `KeyboardEvent` から IchigoJam が解する各種コードへの変換。
//!
//! 物理キー位置 (`KeyboardEvent.code`) で keymap を引くことで、`KBD` コマンドの
//! US/JA 切替が OS の入力レイアウトに依らず効くようにする入り口。

use ichigocrate_core::keycodes as kc;

/// keymap の戻り値のうち REPL 編集を進める制御コード群 (input_control 経由)。
pub(crate) fn is_edit_control_code(c: u8) -> bool {
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
pub(crate) fn fkey_binding(code: &str) -> Option<(&'static str, bool)> {
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
///
/// `"Backslash"` だけ keyboard_id で Usage ID を出し分ける: W3C UI Events
/// Code の仕様上、US の `\` (0x31) と JIS の `]` (0x32) は同じ `"Backslash"`
/// として報告され区別できないため。
pub(crate) fn code_to_hid(code: &str, keyboard_id: u8) -> Option<u8> {
    Some(match code {
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
        "Backslash" => {
            if keyboard_id == 0 {
                0x31
            } else {
                0x32
            }
        }
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
pub(crate) fn code_to_btn_code(code: &str) -> Option<u8> {
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
        assert_eq!(code_to_hid("KeyA", 1), Some(0x04));
        assert_eq!(code_to_hid("KeyZ", 1), Some(0x1d));
        assert_eq!(code_to_hid("Digit2", 1), Some(0x1f));
        assert_eq!(code_to_hid("Digit0", 1), Some(0x27));
        assert_eq!(code_to_hid("BracketLeft", 1), Some(0x2f));
        assert_eq!(code_to_hid("ArrowLeft", 1), Some(0x50));
        assert_eq!(code_to_hid("Enter", 1), None);
    }

    #[test]
    fn code_to_hid_backslash_depends_on_keyboard_id() {
        // US 101 キー配列の `\` = 0x31、JIS 106 キー配列の `]` = 0x32。
        // ブラウザの KeyboardEvent.code は両者を区別せず同じ "Backslash" を
        // 報告するため、KBD で選んだ keyboard_id 側で出し分ける。
        assert_eq!(code_to_hid("Backslash", 0), Some(0x31));
        assert_eq!(code_to_hid("Backslash", 1), Some(0x32));
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
