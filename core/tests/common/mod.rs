//! 結合テスト共通ヘルパー。
//!
//! `tests/` 直下の各テストファイルは個別のテストクレートとしてコンパイルされる。
//! それぞれが `mod common;` でこのモジュールを取り込んで使う。サブディレクトリ
//! 配下のため、このファイル自体は独立したテストバイナリにはならない。
//! (取り込む側ごとに「使っていないヘルパー」が dead_code 扱いになるので許可する)

#![allow(dead_code)]

use ichigocrate_core::ram::IJB_SIZEOF_ARRAY;
use ichigocrate_core::{Machine, OFFSET_RAM_VRAM, SCREEN_W};

/// VRAM 全体 (32x24) を文字列化する。空セル (0) は空白、印字可能 ASCII は
/// そのまま、それ以外 (グラフィック文字など) は `?` に潰す。出力に特定文字列が
/// 含まれるかを assert するのに使う。
pub fn screen_text(m: &Machine) -> String {
    let mut s = String::new();
    let v = &m.ram[OFFSET_RAM_VRAM..OFFSET_RAM_VRAM + 32 * 24];
    for (i, c) in v.iter().enumerate() {
        if i > 0 && i % SCREEN_W == 0 {
            s.push('\n');
        }
        match *c {
            0 => s.push(' '),
            32..=126 => s.push(*c as char),
            _ => s.push('?'),
        }
    }
    s
}

/// VRAM の `y` 行目を、最初の空セル (0) までの文字列として取り出す。
pub fn vram_line(m: &Machine, y: usize) -> String {
    let row = &m.ram[OFFSET_RAM_VRAM + y * SCREEN_W..OFFSET_RAM_VRAM + (y + 1) * SCREEN_W];
    row.iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as char)
        .collect()
}

/// 変数 A-Z の値を読む。内部インデックスは配列領域 (0..102) の後ろに A から
/// 並ぶため `IJB_SIZEOF_ARRAY + (name - 'A')`。
pub fn var(m: &Machine, name: u8) -> i16 {
    m.var_get(IJB_SIZEOF_ARRAY + (name - b'A') as usize)
}
