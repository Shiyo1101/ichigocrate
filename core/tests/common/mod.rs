//! 結合テスト共通ヘルパー。
//!
//! `tests/` 直下の各テストファイルは個別のテストクレートとしてコンパイルされる。
//! それぞれが `mod common;` でこのモジュールを取り込んで使う。サブディレクトリ
//! 配下のため、このファイル自体は独立したテストバイナリにはならない。
//! (取り込む側ごとに「使っていないヘルパー」が dead_code 扱いになるので許可する)

#![allow(dead_code)]

use std::collections::HashMap;

use ichigocrate_core::machine::Storage;
use ichigocrate_core::ram::IJB_SIZEOF_ARRAY;
use ichigocrate_core::{Machine, OFFSET_RAM_VRAM, SCREEN_W, SIZE_RAM_VRAM};

/// VRAM 全体 (32x24) を文字列化する。空セル (0) は空白、印字可能 ASCII は
/// そのまま、それ以外 (グラフィック文字など) は `?` に潰す。出力に特定文字列が
/// 含まれるかを assert するのに使う。
pub fn screen_text(m: &Machine) -> String {
    let mut s = String::new();
    for (i, c) in vram_bytes(m).iter().enumerate() {
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

/// スロットごとに保存内容を保持するメモリ上の `Storage`。SAVE/LOAD の
/// ラウンドトリップ検証用。未保存スロットの LOAD は失敗 (`None`) を返す。
#[derive(Debug, Default)]
pub struct MemStore {
    slots: HashMap<u8, Vec<u8>>,
}

impl Storage for MemStore {
    fn save(&mut self, slot: u8, data: &[u8]) -> bool {
        self.slots.insert(slot, data.to_vec());
        true
    }

    fn load(&mut self, slot: u8, buf: &mut [u8]) -> Option<usize> {
        let data = self.slots.get(&slot)?;
        let n = data.len().min(buf.len());
        buf[..n].copy_from_slice(&data[..n]);
        buf[n..].fill(0);
        Some(n)
    }

    fn peek(&mut self, slot: u8, buf: &mut [u8]) -> Option<usize> {
        self.load(slot, buf)
    }
}

/// VRAM 全体 (32x24) の生バイト列。グラフィック文字の有無を調べるのに使う。
pub fn vram_bytes(m: &Machine) -> &[u8] {
    &m.ram[OFFSET_RAM_VRAM..OFFSET_RAM_VRAM + SIZE_RAM_VRAM]
}

/// `haystack` に `needle` が連続して現れるか。
pub fn contains_seq(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
