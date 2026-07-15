//! SAVE/LOAD/FILES の Web 実装。

use std::collections::HashMap;

use base64::Engine;
use ichigocrate_core::machine::Storage;

/// 提供するセーブスロット数 (実機 IchigoJam 準拠)。
const SLOT_COUNT: u8 = 16;

/// `persist=true` は localStorage、false はセッション内のみの揮発メモリへ保存する。
/// `prefix` で複数インスタンスのスロットを分離する。localStorage が使えない
/// (プライベートモード等) ときは揮発メモリへフォールバックする。
#[derive(Debug)]
pub(crate) struct WebStorage {
    prefix: String,
    is_persistent: bool,
    mem: HashMap<u8, Vec<u8>>,
}

impl WebStorage {
    pub(crate) fn new(prefix: String, is_persistent: bool) -> Self {
        Self {
            prefix,
            is_persistent,
            mem: HashMap::new(),
        }
    }

    fn key(&self, slot: u8) -> String {
        format!("{}slot_{:02}", self.prefix, slot)
    }

    /// 永続化が有効で localStorage が使えるときだけハンドルを返す。
    fn local_storage(&self) -> Option<web_sys::Storage> {
        if !self.is_persistent {
            return None;
        }
        web_sys::window()?.local_storage().ok().flatten()
    }
}

impl Storage for WebStorage {
    fn save(&mut self, slot: u8, data: &[u8]) -> bool {
        if let Some(ls) = self.local_storage() {
            let encoded = base64::engine::general_purpose::STANDARD.encode(data);
            ls.set_item(&self.key(slot), &encoded).is_ok()
        } else {
            self.mem.insert(slot, data.to_vec());
            true
        }
    }

    fn load(&mut self, slot: u8, buf: &mut [u8]) -> Option<usize> {
        let data = if let Some(ls) = self.local_storage() {
            let s = ls.get_item(&self.key(slot)).ok().flatten()?;
            base64::engine::general_purpose::STANDARD.decode(s).ok()?
        } else {
            self.mem.get(&slot)?.clone()
        };
        let n = data.len().min(buf.len());
        buf[..n].copy_from_slice(&data[..n]);
        // 残りはゼロ埋め (リスト終端を保証)。
        buf[n..].fill(0);
        Some(n)
    }

    fn peek(&mut self, slot: u8, buf: &mut [u8]) -> Option<usize> {
        self.load(slot, buf)
    }

    fn slot_count(&self) -> u8 {
        SLOT_COUNT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_storage_volatile_roundtrip() {
        // persist=false は localStorage を触らず揮発メモリで往復する
        // (ネイティブテストでも window 無しで動く)。
        let mut s = WebStorage::new("test-".into(), false);
        assert!(s.save(3, &[1, 2, 3, 4]));
        let mut buf = [0u8; 8];
        assert_eq!(s.load(3, &mut buf), Some(4));
        assert_eq!(&buf[..4], &[1, 2, 3, 4]);
        assert_eq!(&buf[4..], &[0, 0, 0, 0]); // 残りゼロ埋め
        assert_eq!(s.load(9, &mut buf), None); // 未保存スロット
    }
}
