//! 貼り付けたテキストを、IchigoJam で打ち込める形に整える。
//!
//! ホストのクリップボードから受け取った文字列 (UTF-8) を IchigoJam の文字コードへ
//! 変換し、[`crate::session::Session::queue_paste`] でキー入力として流し込める
//! バイト列にする。あわせて、確認画面に出すための情報 (行数・注意点) を集める。

use crate::ram::N_LINEBUF;

/// 1 行に打ち込める最大の文字数 (行バッファの大きさ - 終端)。
pub const MAX_LINE_CHARS: usize = N_LINEBUF - 1;

/// 半角カナ (U+FF61〜U+FF9F) が IchigoJam の文字コードで始まる位置 (0xA1〜0xDF)。
const HALFWIDTH_KANA_FIRST: u32 = 0xFF61;
const HALFWIDTH_KANA_LAST: u32 = 0xFF9F;
const ICHIGOJAM_KANA_FIRST: u8 = 0xA1;

/// 貼り付けの準備結果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreparedPaste {
    /// 流し込むバイト列。各行は `\n` で終わる (空行は含まない)。
    pub bytes: Vec<u8>,
    /// 行数 (空行を除く)。
    pub lines: usize,
    /// 行番号で始まる行 (プログラムとして登録される行) の数。
    pub numbered_lines: usize,
    /// 行番号の無い行。打ち込むとその場で実行される (`RUN` や `NEW` など)。
    pub immediate_lines: Vec<String>,
    /// [`MAX_LINE_CHARS`] を超える行の数。行の後ろが切れる。
    pub long_lines: usize,
    /// IchigoJam で表せないため飛ばした文字の数。
    pub skipped_chars: usize,
}

impl PreparedPaste {
    /// 流し込むものが何も無いか。
    pub fn is_empty(&self) -> bool {
        self.lines == 0
    }
}

/// 貼り付けたテキストを整える。
///
/// - 改行は CRLF / CR / LF のどれでもよい。空行と行末の空白は捨てる
/// - タブは空白にする
/// - 印字可能な ASCII はそのまま、半角カナは IchigoJam のカナの文字コードにする。
///   それ以外 (全角文字・絵文字・制御文字など) は飛ばして数える
pub fn prepare_paste(text: &str) -> PreparedPaste {
    let mut out = PreparedPaste::default();
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    for raw in normalized.split('\n') {
        let mut line = Vec::with_capacity(raw.len());
        for c in raw.chars() {
            match to_ichigojam(c) {
                Some(b) => line.push(b),
                None => out.skipped_chars += 1,
            }
        }
        while line.last() == Some(&b' ') {
            line.pop();
        }
        if line.is_empty() {
            continue;
        }
        out.lines += 1;
        if line.len() > MAX_LINE_CHARS {
            out.long_lines += 1;
        }
        let first = line.iter().find(|&&b| b != b' ');
        if first.is_some_and(u8::is_ascii_digit) {
            out.numbered_lines += 1;
        } else {
            out.immediate_lines.push(
                line.iter()
                    .map(|&b| char::from(b))
                    .collect::<String>()
                    .trim()
                    .to_string(),
            );
        }
        out.bytes.extend_from_slice(&line);
        out.bytes.push(b'\n');
    }
    out
}

/// 1 文字を IchigoJam の文字コードにする。表せなければ `None`。
fn to_ichigojam(c: char) -> Option<u8> {
    match c {
        '\t' => Some(b' '),
        ' '..='~' => Some(c as u8),
        _ => {
            let code = u32::from(c);
            (HALFWIDTH_KANA_FIRST..=HALFWIDTH_KANA_LAST)
                .contains(&code)
                .then(|| ICHIGOJAM_KANA_FIRST + (code - HALFWIDTH_KANA_FIRST) as u8)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_newlines_and_drops_blank_lines() {
        let p = prepare_paste("10 ?\"A\"\r\n\r\n20 END  \r30 GOTO 10\n\n");
        assert_eq!(p.bytes, b"10 ?\"A\"\n20 END\n30 GOTO 10\n");
        assert_eq!((p.lines, p.numbered_lines), (3, 3));
        assert!(p.immediate_lines.is_empty());
    }

    #[test]
    fn lines_without_numbers_are_reported() {
        let p = prepare_paste("OK\n10 ?1\n  RUN\n");
        assert_eq!(p.numbered_lines, 1);
        assert_eq!(p.immediate_lines, ["OK", "RUN"]);
        // 行頭の空白は残す (そのまま打ち込む)
        assert_eq!(p.bytes, b"OK\n10 ?1\n  RUN\n");
    }

    #[test]
    fn converts_halfwidth_kana_and_skips_unsupported() {
        // ｱ (U+FF71) → 0xB1、ﾟ (U+FF9F) → 0xDF。全角「あ」と絵文字は飛ばす
        let p = prepare_paste("10 ?\"ｱﾟあ😀\"\t'X");
        assert_eq!(
            p.bytes,
            [b"10 ?\"".as_slice(), &[0xB1, 0xDF], b"\" 'X\n"].concat()
        );
        assert_eq!(p.skipped_chars, 2);
    }

    #[test]
    fn counts_long_lines() {
        let long = format!("10 '{}", "A".repeat(MAX_LINE_CHARS));
        let p = prepare_paste(&format!("{long}\n20 END"));
        assert_eq!(p.long_lines, 1);
        assert_eq!(p.lines, 2);
    }

    #[test]
    fn empty_or_unsupported_only_is_empty() {
        assert!(prepare_paste("").is_empty());
        assert!(prepare_paste("\n \n\t\n").is_empty());
        assert!(prepare_paste("あいう").is_empty());
    }
}
