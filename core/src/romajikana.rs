//! ローマ字 → カタカナ (半角) 入力変換。
//!
//! - 半角カタカナ (JIS X 0201) を出力する。0xB1-0xDF が「ア」～「゜」。
//! - 画面に出したローマ字を BS (0x08) で消し、確定したカナに
//!   差し替える動きも含めて再現する。
//! - 入力の途中経過を 2 文字分 (`buf0`, `buf1`) だけ覚えておく。
//! - 1 入力あたりの出力は最大 5 バイト (例: `DHA` → BS, BS, テ, ゛, ャ)。
//!   ヒープ確保を避けるため固定サイズの [`KanaOutput`] を返す。

/// `romajikana_input` の出力。最大 5 バイトの半固定列。
#[derive(Debug, Clone, Copy, Default)]
pub struct KanaOutput {
    bytes: [u8; 6],
    len: u8,
}

impl KanaOutput {
    fn push(&mut self, b: u8) {
        let i = self.len as usize;
        debug_assert!(i < self.bytes.len(), "KanaOutput overflow");
        self.bytes[i] = b;
        self.len += 1;
    }

    /// 出力済みのスライスを返す。
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    /// 出力が空かどうか。
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<'a> IntoIterator for &'a KanaOutput {
    type Item = u8;
    type IntoIter = std::iter::Copied<std::slice::Iter<'a, u8>>;
    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter().copied()
    }
}

/// 1 文字目 `buf0` を覚えている状態で、次に来た英字 `k` を 2 文字目 `buf1`
/// として覚えてよいかを判定する。
///
/// `buf0` + `k` + 母音 でカナにできる並び
/// (`*Y` / `CH` `SH` `TH` `DH` `WH` / `TS` / `LT` `XT`) のときだけ `true` を返す。
/// ここで確かめておかないと、"DJ" のように「つなげてもカナにできない並び」を
/// 2 文字目として覚えてしまい、続く母音入力で画面の文字が丸ごと消える
/// 不具合 (例: "DJI" が "Dジ" にならず全部消える) につながる。
///
/// `Y` は移植元がどの文字でも `*Y` の並びとして扱うため、動作を変えないよう
/// 常に許可する。
fn kana_pair_can_combine(buf0: u8, k: u8) -> bool {
    match k {
        b'Y' => true,
        b'H' => matches!(buf0, b'C' | b'S' | b'T' | b'D' | b'W'), // CH/SH/TH/DH/WH
        b'S' => buf0 == b'T',                              // TS → ツ
        b'T' => matches!(buf0, b'L' | b'X'),               // LT/XT → ッ
        _ => false,
    }
}

/// 1 文字の入力を受け取り、画面に流すべきバイト列を返す。
///
/// 返却される列にはバックスペース (0x08) や濁点 (0xDE)、半濁点 (0xDF) も
/// 含まれる。呼び出し側は順にスクリーンへ流せばよい。
///
/// `buf0` / `buf1` は呼び出し側が持つ「まだカナになっていないローマ字を
/// 覚えておく入れ物」(初期値 0)。
pub fn romajikana_input(buf0: &mut u8, buf1: &mut u8, k_in: u8) -> KanaOutput {
    let mut k: u8 = k_in;
    if k.is_ascii_lowercase() {
        k = k - b'a' + b'A';
    }

    let mut m: i32 = -1;
    match k {
        b'A' => m = 0,
        b'I' => m = 1,
        b'U' => m = 2,
        b'E' => m = 3,
        b'O' => m = 4,
        b'.' => k = 0xa1,
        b',' => k = 0xa4,
        b'-' => k = 0xb0,
        b'[' => k = 0xa2,
        b']' => k = 0xa3,
        b'/' => k = 0xa5,
        b'\\' => k = 0xa0, // yen mark
        _ => {}
    }

    let a2z = k.is_ascii_uppercase();
    let mut out = KanaOutput::default();

    if *buf0 == 0 {
        if m >= 0 {
            k = 0xb1 + m as u8; // アイウエオ
        } else if a2z {
            *buf0 = k;
            *buf1 = 0;
        }
    } else if *buf1 == 0 {
        if m >= 0 {
            out.push(8); // back space
            match *buf0 {
                b'K' | b'C' => k = 0xb6 + m as u8,
                b'S' => k = 0xbb + m as u8,
                b'T' => k = 0xc0 + m as u8,
                b'N' => k = 0xc5 + m as u8,
                b'H' => k = 0xca + m as u8,
                b'M' => k = 0xcf + m as u8,
                b'Y' => {
                    if (m & 1) == 0 {
                        k = 0xd4 + (m as u8 >> 1);
                    } else {
                        k = 0xb1 + m as u8; // _イ_エ_
                    }
                }
                b'R' => k = 0xd7 + m as u8,
                b'W' => {
                    if m == 0 {
                        k = 0xdc;
                    } else if m == 2 {
                        k = 0xb3;
                    } else if m == 4 {
                        k = 0xa6;
                    } else {
                        out.push(0xb3);
                        k = 0xa7 + m as u8;
                    }
                }
                b'L' | b'X' => k = 0xa7 + m as u8,
                b'G' => {
                    out.push(0xb6 + m as u8);
                    k = 0xde;
                }
                b'Z' => {
                    out.push(0xbb + m as u8);
                    k = 0xde;
                }
                b'J' => {
                    out.push(0xbc);
                    out.push(0xde);
                    if m == 1 {
                        k = 0;
                    } else if m == 3 {
                        k = 0xaa;
                    } else {
                        k = 0xac + (m as u8 >> 1);
                    }
                }
                b'F' => {
                    out.push(0xcc);
                    if m == 2 {
                        k = 0;
                    } else {
                        k = 0xa7 + m as u8;
                    }
                }
                b'V' => {
                    out.push(0xb3);
                    out.push(0xde);
                    if m == 2 {
                        k = 0;
                    } else {
                        k = 0xa7 + m as u8;
                    }
                }
                b'D' => {
                    out.push(0xc0 + m as u8);
                    k = 0xde;
                }
                b'B' => {
                    out.push(0xca + m as u8);
                    k = 0xde;
                }
                b'P' => {
                    out.push(0xca + m as u8);
                    k = 0xdf;
                }
                b'Q' => {
                    out.push(0xb8);
                    k = 0xa7 + m as u8;
                }
                _ => {}
            }
            *buf0 = 0;
        } else if k == *buf0 {
            out.push(8);
            if k == b'N' {
                k = 0xdd; // ン
                *buf0 = 0;
            } else {
                out.push(0xaf); // ッ
            }
        } else if *buf0 == b'N' && k != b'Y' {
            out.push(8);
            out.push(0xdd); // ン
            if a2z {
                *buf0 = k;
            } else {
                *buf0 = 0;
                if k == 0x27 {
                    // '
                    k = 0;
                }
            }
        } else if a2z {
            // buf0 + k が母音を続けてカナにできる並び
            // (`*Y` や CH/SH/TH/DH/WH/TS/LT/XT) のときだけ 2 文字目 buf1 として覚える。
            //
            // Why: 移植元は英字なら中身を確かめずに buf1=k と覚えていた。
            // しかし "DJ" のように「つなげてもカナにできない並び」を
            // 覚えると、続く母音入力で先頭の `m >= 0` 分岐がバックスペースを 2 個
            // 出して画面の "DJ" を両方消したあと、どのカナにも当てはまらず
            // k=0 となり、画面の文字が丸ごと消えてしまう
            // (例: "DJI" が "Dジ" にならず全部消える)。
            // カナにできない並びのときは 1 文字目をローマ字のまま画面に残して
            // 確定し、k を新しい 1 文字目 buf0 として覚え直す。
            if kana_pair_can_combine(*buf0, k) {
                *buf1 = k;
            } else {
                *buf0 = k;
            }
        } else {
            *buf0 = 0;
        }
    } else if m >= 0 {
        out.push(8);
        out.push(8);
        if (*buf0 == b'C' || *buf0 == b'S') && *buf1 == b'H' {
            // ちゃちちゅちぇちょ / しゃししゅしぇしょ
            out.push(if *buf0 == b'C' { 0xc1 } else { 0xbc });
            if m == 1 {
                k = 0;
            } else if m == 3 {
                k = 0xaa;
            } else {
                k = 0xac + (m as u8 >> 1);
            }
        } else if *buf0 == b'T' && *buf1 == b'S' {
            // TSU → ツ。現代の日本語入力では TSA/TSI/TSE/TSO は
            // ツ + 小さい母音 (ツァ/ツィ/ツェ/ツォ) になる。
            // FA → ファ (フ + 小さい ァ) と同じく、ツ を出したあと
            // 小さい母音 (0xa7 + m) を続ける。
            if m == 2 {
                k = 0xc2; // ツ
            } else {
                out.push(0xc2); // ツ
                k = 0xa7 + m as u8; // ァ ィ ェ ォ
            }
        } else if *buf0 == b'W' && *buf1 == b'H' {
            // 現代の日本語入力では WHA/WHI/WHE/WHO は
            // ウ + 小さい母音 (ウァ/ウィ/ウェ/ウォ) になる。
            // WHU は ウ のみ (FU → フ と同じ)。
            out.push(0xb3); // ウ
            if m == 2 {
                k = 0;
            } else {
                k = 0xa7 + m as u8; // ァ ィ ェ ォ
            }
        } else if (*buf0 == b'L' || *buf0 == b'X') && *buf1 == b'T' && m == 2 {
            k = 0xaf; // ッ
        } else if (*buf0 == b'T' || *buf0 == b'D') && *buf1 == b'H' {
            out.push(0xc3); // テ
            if *buf0 == b'D' {
                out.push(0xde);
            }
            if (m & 1) == 0 {
                k = 0xac + (m as u8 >> 1);
            } else {
                k = 0xa7 + m as u8;
            }
        } else if *buf1 == b'Y' {
            match *buf0 {
                b'K' | b'C' => out.push(0xb6 + 1),
                b'S' => out.push(0xbb + 1),
                b'T' => out.push(0xc0 + 1),
                b'N' => out.push(0xc5 + 1),
                b'H' => out.push(0xca + 1),
                b'F' => out.push(0xca + 2),
                b'J' => {
                    out.push(0xbc);
                    out.push(0xde);
                }
                b'M' => out.push(0xcf + 1),
                b'R' => out.push(0xd7 + 1),
                b'G' => {
                    out.push(0xb6 + 1);
                    out.push(0xde);
                }
                b'Z' => {
                    out.push(0xbb + 1);
                    out.push(0xde);
                }
                b'D' => {
                    out.push(0xc0 + 1);
                    out.push(0xde);
                }
                b'B' => {
                    out.push(0xca + 1);
                    out.push(0xde);
                }
                b'P' => {
                    out.push(0xca + 1);
                    out.push(0xdf);
                }
                _ => {}
            }
            if (m & 1) == 0 {
                k = 0xac + (m as u8 >> 1);
            } else {
                k = 0xa7 + m as u8;
            }
        } else {
            k = 0;
        }
        *buf0 = 0;
        *buf1 = 0;
    } else if a2z {
        *buf0 = *buf1;
        *buf1 = k;
    } else if k == 8 {
        *buf1 = 0;
    } else {
        *buf0 = 0;
    }

    if k != 0 {
        out.push(k);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_str(s: &str) -> Vec<u8> {
        let mut buf0 = 0u8;
        let mut buf1 = 0u8;
        let mut emitted: Vec<u8> = Vec::new();
        for c in s.bytes() {
            let r = romajikana_input(&mut buf0, &mut buf1, c);
            for b in &r {
                if b == 8 {
                    emitted.pop();
                } else {
                    emitted.push(b);
                }
            }
        }
        emitted
    }

    #[test]
    fn vowels() {
        // ア イ ウ エ オ
        assert_eq!(type_str("AIUEO"), vec![0xb1, 0xb2, 0xb3, 0xb4, 0xb5]);
    }

    #[test]
    fn ka_row() {
        // カ キ ク ケ コ
        assert_eq!(type_str("KAKIKUKEKO"), vec![0xb6, 0xb7, 0xb8, 0xb9, 0xba]);
    }

    #[test]
    fn shi_chi_tsu() {
        // シ チ ツ
        assert_eq!(type_str("SHI"), vec![0xbc]);
        assert_eq!(type_str("CHI"), vec![0xc1]);
        assert_eq!(type_str("TSU"), vec![0xc2]);
    }

    #[test]
    fn n_double() {
        // NN → ン
        assert_eq!(type_str("NN"), vec![0xdd]);
        // NA → ナ
        assert_eq!(type_str("NA"), vec![0xc5]);
        // NK の連続: N が独立 → ン に確定し、その後 K がバッファに残る
        // NKA → ンカ
        assert_eq!(type_str("NKA"), vec![0xdd, 0xb6]);
    }

    #[test]
    fn small_tsu() {
        // KKA → ッカ
        assert_eq!(type_str("KKA"), vec![0xaf, 0xb6]);
        // TTA → ッタ
        assert_eq!(type_str("TTA"), vec![0xaf, 0xc0]);
    }

    #[test]
    fn dakuten_handakuten() {
        // GA → カ゛
        assert_eq!(type_str("GA"), vec![0xb6, 0xde]);
        // PA → ハ゜
        assert_eq!(type_str("PA"), vec![0xca, 0xdf]);
        // BA → ハ゛
        assert_eq!(type_str("BA"), vec![0xca, 0xde]);
        // ZA → サ゛
        assert_eq!(type_str("ZA"), vec![0xbb, 0xde]);
        // DA → タ゛
        assert_eq!(type_str("DA"), vec![0xc0, 0xde]);
    }

    #[test]
    fn youon() {
        // KYA → キャ
        assert_eq!(type_str("KYA"), vec![0xb7, 0xac]);
        // SHA → シャ
        assert_eq!(type_str("SHA"), vec![0xbc, 0xac]);
        // CHA → チャ
        assert_eq!(type_str("CHA"), vec![0xc1, 0xac]);
        // JA → ジャ (シ゛ャ)
        assert_eq!(type_str("JA"), vec![0xbc, 0xde, 0xac]);
    }

    #[test]
    fn fa_fi_fe_fo() {
        // FA → フ ァ
        assert_eq!(type_str("FA"), vec![0xcc, 0xa7]);
        // FI → フ ィ
        assert_eq!(type_str("FI"), vec![0xcc, 0xa8]);
        // FU → フ
        assert_eq!(type_str("FU"), vec![0xcc]);
    }

    #[test]
    fn wa_wo() {
        // WA → ワ
        assert_eq!(type_str("WA"), vec![0xdc]);
        // WO → ヲ
        assert_eq!(type_str("WO"), vec![0xa6]);
        // WI → ウィ
        assert_eq!(type_str("WI"), vec![0xb3, 0xa8]);
    }

    #[test]
    fn tsa_tsi_tse_tso() {
        // 現代ローマ字: TSU は従来どおりツ、TSA/TSI/TSE/TSO は ツ + 小書き母音。
        assert_eq!(type_str("TSU"), vec![0xc2]); // ツ
        assert_eq!(type_str("TSA"), vec![0xc2, 0xa7]); // ツァ
        assert_eq!(type_str("TSI"), vec![0xc2, 0xa8]); // ツィ
        assert_eq!(type_str("TSE"), vec![0xc2, 0xaa]); // ツェ
        assert_eq!(type_str("TSO"), vec![0xc2, 0xab]); // ツォ
    }

    #[test]
    fn wha_whi_whu_whe_who() {
        // 現代ローマ字: WHU は ウ のみ、WHA/WHI/WHE/WHO は ウ + 小書き母音。
        assert_eq!(type_str("WHU"), vec![0xb3]); // ウ
        assert_eq!(type_str("WHA"), vec![0xb3, 0xa7]); // ウァ
        assert_eq!(type_str("WHI"), vec![0xb3, 0xa8]); // ウィ
        assert_eq!(type_str("WHE"), vec![0xb3, 0xaa]); // ウェ
        assert_eq!(type_str("WHO"), vec![0xb3, 0xab]); // ウォ
    }

    #[test]
    fn xtu_ltu() {
        // XTU → ッ
        assert_eq!(type_str("XTU"), vec![0xaf]);
        // LTU → ッ
        assert_eq!(type_str("LTU"), vec![0xaf]);
    }

    #[test]
    fn invalid_consonant_pair_keeps_orphan() {
        // 結合し得ない子音対は先頭子音をローマ字のまま残し、後半をカナ化する。
        // "DJI": D は孤立した 'D'、"JI" → ジ (シ゛) で "Dジ" になる。
        // (以前は BS×2 + k=0 で "DJI" が丸ごと消えていた)
        assert_eq!(type_str("DJI"), vec![b'D', 0xbc, 0xde]);
        // "KHA": K は孤立、"HA" → ハ。
        assert_eq!(type_str("KHA"), vec![b'K', 0xca]);
        // "KSA": K は孤立、"SA" → サ。
        assert_eq!(type_str("KSA"), vec![b'K', 0xbb]);
    }

    #[test]
    fn punctuation() {
        // . , - / [ ] と \ をカタカナ記号にマップ
        assert_eq!(type_str("."), vec![0xa1]); // 。
        assert_eq!(type_str(","), vec![0xa4]); // 、
        assert_eq!(type_str("-"), vec![0xb0]); // ー
        assert_eq!(type_str("["), vec![0xa2]);
        assert_eq!(type_str("]"), vec![0xa3]);
        assert_eq!(type_str("/"), vec![0xa5]);
    }
}
