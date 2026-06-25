//! onPrint (画面出力ストリーミング) を支える VRAM 差分の純粋ヘルパ。
//!
//! core を一切改変せず、フレーム間の VRAM スナップショット差分で PRINT 出力を
//! 近似するための小さな部品を集める。状態を持つ抽出本体は [`crate::runner`] 側。

/// VRAM の 1 バイトを onPrint 用の文字へ変換する (印字不能・グラフィックは `?`)。
pub(crate) fn screen_char(c: u8) -> char {
    match c {
        0 => ' ',
        32..=126 => c as char,
        _ => '?',
    }
}

/// 直前フレームからのスクロール量 (行数) を検出する。`cur` の上側 (rows-k) 行が
/// `prev` の下側に一致する最小の k を返す。一致しなければ 0。1 フレーム内に複数行
/// スクロールしてもここで検出できる範囲は補正する (画面外へ流れ切った分は不可)。
pub(crate) fn detect_scroll(prev: &[u8], cur: &[u8], cols: usize, rows: usize) -> usize {
    if prev.len() != cur.len() || cols == 0 || rows == 0 {
        return 0;
    }
    for k in 1..rows {
        let n = (rows - k) * cols;
        if cur[..n] == prev[k * cols..k * cols + n] {
            return k;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_scroll_finds_single_and_multi_line_shift() {
        let cols = 4;
        let rows = 3;
        // prev: 行 0=AAAA 1=BBBB 2=CCCC
        let prev = b"AAAABBBBCCCC".to_vec();
        // 1 行スクロール: 上が BBBB CCCC に詰まり末尾が新規。
        let mut cur = b"BBBBCCCC????".to_vec();
        assert_eq!(detect_scroll(&prev, &cur, cols, rows), 1);
        // スクロールなし。
        assert_eq!(detect_scroll(&prev, &prev, cols, rows), 0);
        // 2 行スクロール。
        cur = b"CCCC????????".to_vec();
        assert_eq!(detect_scroll(&prev, &cur, cols, rows), 2);
    }
}
