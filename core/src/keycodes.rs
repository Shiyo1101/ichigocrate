//! IchigoJam の制御コード / キーコード定数。
//!
//! 画面エディタ (`screen`)、ホストのキーボード入力 (app)、`BTN()` が同じ
//! ASCII 値を参照する。生の数値を各所に直書きすると値がずれても気付け
//! ないため、唯一の定義元としてここに集約する。

/// カーソル左移動 (←)
pub const CURSOR_LEFT: u8 = 28;
/// カーソル右移動 (→)
pub const CURSOR_RIGHT: u8 = 29;
/// カーソル上移動 (↑)
pub const CURSOR_UP: u8 = 30;
/// カーソル下移動 (↓)
pub const CURSOR_DOWN: u8 = 31;
/// スペース
pub const SPACE: u8 = b' ';
/// バックスペース (1 文字戻って詰める)
pub const BACKSPACE: u8 = 0x08;
/// デリート (カーソル位置の文字を詰める)
pub const DELETE: u8 = 0x7f;
/// タブ
pub const TAB: u8 = b'\t';
/// 行頭へ (Home)
pub const HOME: u8 = 0x12;
/// 行末へ (End)
pub const END: u8 = 0x17;
/// 画面先頭へ (PageUp)
pub const PAGE_UP: u8 = 0x13;
/// 画面末尾へ (PageDown)
pub const PAGE_DOWN: u8 = 0x14;
/// 書式送り: カーソル以降を消去 (FF)
pub const FORM_FEED: u8 = 12;
/// カナモード切替
pub const KANA_TOGGLE: u8 = 15;
/// 挿入/上書きモード切替
pub const INSERT_TOGGLE: u8 = 17;
/// 行分割 (本移植では改行扱い)
pub const LINE_SPLIT: u8 = 0x10;
/// LOCATE 連動シーケンス開始 (続く 2 文字を座標として解釈)
pub const LOCATE_PREFIX: u8 = 21;
/// X キー。`BTN()` 既定の代用ボタン (実機ボタンの代わり)。
pub const KEY_X: u8 = b'X';
