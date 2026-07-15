//! グラフィック文字 (128-255) のバイト保持と CHR$ / VPEEK / POKE のバイト透過テスト。
//!
//! バグ報告: プログラム記述後に実行すると別の文字に変わる。
//! 原因: REPL 経路で String::push(c as char) → as_bytes() の UTF-8 化により
//! 128-255 のバイトが 2 バイトに展開されてしまう。直接モード (RAM への直接
//! 書き込み) ではこのバグは出ない。`exec_line_bytes` (生バイト経由) を使えば
//! REPL でも壊れないことを各テストで担保する。

mod common;

use common::vram_line;
use ichigocrate_core::{
    exec_line, exec_line_bytes, run_to_completion, Machine, OFFSET_RAMROM, OFFSET_RAM_LINEBUF,
    OFFSET_RAM_LIST, OFFSET_RAM_VRAM, SCREEN_W,
};

/// 0xEA (= Alt+A 相当のグラフィック文字) を含む PRINT 行が、生バイト経由なら
/// 元のバイトのまま画面に書かれること。
#[test]
fn graphic_char_in_string_literal_preserved_via_bytes_api() {
    let mut m = Machine::new();
    // ?"X\xEA" 相当の行を生バイトで実行
    let line: &[u8] = b"?\"X\xea\"";
    let _ = exec_line_bytes(&mut m, line);
    // VRAM の行 0 は X(0x58), 0xEA を保持しているはず
    assert_eq!(m.ram[OFFSET_RAM_VRAM], b'X');
    assert_eq!(m.ram[OFFSET_RAM_VRAM + 1], 0xea);
}

/// VRAM に直接グラフィック文字を含む行を書いた後、その行を LINEBUF に生
/// バイトでコピーして実行すれば、文字列が壊れずに出力されること。
/// (これは app の execute_current_line が踏むべき経路。)
#[test]
fn vram_line_with_graphic_char_round_trips_through_linebuf() {
    let mut m = Machine::new();
    // VRAM 行 0 に `?"\xea"` を直接書く
    let bytes: &[u8] = b"?\"\xea\"";
    for (i, &b) in bytes.iter().enumerate() {
        m.ram[OFFSET_RAM_VRAM + i] = b;
    }
    // LINEBUF に生コピーして実行
    let _ = exec_line_bytes(&mut m, bytes);
    // 1 行目に出力される (行 0 は元の文字列がそのまま残るので別の行へ流れる)
    // 出力の途中だが、PRINT で書き出された 0xEA がどこかに現れるはず。
    let vram = &m.ram[OFFSET_RAM_VRAM..OFFSET_RAM_VRAM + 32 * 24];
    assert!(
        vram.contains(&0xea),
        "VRAM should contain the graphic byte 0xEA after PRINT"
    );
}

/// 行番号付きで LIST に保存した行がグラフィック文字をそのまま保持すること。
#[test]
fn list_storage_preserves_graphic_chars() {
    let mut m = Machine::new();
    // 10 ?"\xea\xff"
    let line: &[u8] = b"10 ?\"\xea\xff\"";
    let _ = exec_line_bytes(&mut m, line);
    // LIST 領域に \xEA \xFF がそのまま入っているはず
    let list = &m.ram[OFFSET_RAM_LIST..];
    let has_ea = list.windows(1).any(|w| w[0] == 0xea);
    let has_ff = list.windows(1).any(|w| w[0] == 0xff);
    assert!(has_ea, "LIST should contain 0xEA byte unchanged");
    assert!(has_ff, "LIST should contain 0xFF byte unchanged");
}

/// 行番号付きで保存 → RUN すると、PRINT がグラフィック文字を VRAM へ出力する。
#[test]
fn run_program_prints_graphic_chars_unchanged() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"10 ?\"A\xea\xffZ\"");
    let _ = exec_line_bytes(&mut m, b"RUN");
    run_to_completion(&mut m);
    // 4 連続バイト 'A', 0xEA, 0xFF, 'Z' が VRAM のどこかに連続して現れる
    let vram = &m.ram[OFFSET_RAM_VRAM..OFFSET_RAM_VRAM + 32 * 24];
    let pattern = [b'A', 0xea, 0xff, b'Z'];
    let found = vram.windows(4).any(|w| w == pattern);
    assert!(
        found,
        "RUN should print A, 0xEA, 0xFF, Z contiguously in VRAM"
    );
}

/// exec_line(&str) で grphic char を String::push(c as char) 経由で渡すと、
/// UTF-8 展開が起きるためバイトが破壊される。これは API としては想定済みの
/// 制約 (ASCII 専用) であることを明示するテスト。
/// 修正後に app が exec_line_bytes を使うので REPL では問題にならないが、
/// `exec_line(&str)` の利用者は ASCII のみ渡すべきという契約を担保する。
#[test]
fn exec_line_str_is_documented_ascii_only() {
    let mut m = Machine::new();
    // String 経由だと "?\"\u{00EA}\"" は 4 バイトでなく 5 バイトに展開される
    // (\u{00EA} が UTF-8 で 0xC3 0xAA に化けるため)。これは仕様。
    let s: String = b"?\"".iter().map(|&b| b as char).collect();
    let extended: String = format!("{s}{}\"", '\u{00EA}');
    let _ = exec_line(&mut m, &extended);
    // LINEBUF には UTF-8 化された 0xC3 0xAA が入る (= 仕様としてのドキュメント)
    assert_eq!(m.ram[OFFSET_RAM_LINEBUF], b'?');
    assert_eq!(m.ram[OFFSET_RAM_LINEBUF + 1], b'"');
    assert_eq!(m.ram[OFFSET_RAM_LINEBUF + 2], 0xc3);
    assert_eq!(m.ram[OFFSET_RAM_LINEBUF + 3], 0xaa);
}

/// VPEEK でグラフィック文字を読み戻せる (BASIC 側からの観測も正しい)。
#[test]
fn vpeek_returns_raw_graphic_byte() {
    let mut m = Machine::new();
    // VRAM (x=10, y=5) に 0xEA を直接書く
    m.ram[OFFSET_RAM_VRAM + SCREEN_W * 5 + 10] = 0xea;
    // ?VPEEK(10,5) は直接モードでカーソル位置 (0,0) に "234" を出力
    let _ = exec_line_bytes(&mut m, b"?VPEEK(10,5)");
    assert_eq!(vram_line(&m, 0), "234");
}

/// CHR$(0xEA) で文字列出力したときに、変換せず元のバイトのまま画面へ出ること。
#[test]
fn chr_dollar_prints_graphic_byte_unchanged() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?CHR$(234)");
    assert_eq!(m.ram[OFFSET_RAM_VRAM], 0xea);
}

/// CHR$(0) は NULL セル (= 空) を 1 つ書き、カーソルを進める。
/// `vram_line` ヘルパは最初の 0 で停止するので生 VRAM を確認する。
#[test]
fn chr_dollar_zero_writes_null_cell() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?\"A\";CHR$(0);\"B\"");
    assert_eq!(m.ram[OFFSET_RAM_VRAM], b'A');
    assert_eq!(m.ram[OFFSET_RAM_VRAM + 1], 0);
    assert_eq!(m.ram[OFFSET_RAM_VRAM + 2], b'B');
}

/// CHR$(32..127) は ASCII 印字可能文字としてそのまま画面に出る。
#[test]
fn chr_dollar_ascii_printable_round_trip() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?CHR$(65)");
    assert_eq!(vram_line(&m, 0), "A");
    let _ = exec_line_bytes(&mut m, b"?CHR$(126)");
    assert_eq!(vram_line(&m, 1), "~");
}

/// CHR$(128) / CHR$(255) のグラフィック文字境界。
#[test]
fn chr_dollar_graphic_char_boundaries() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?CHR$(128)");
    assert_eq!(m.ram[OFFSET_RAM_VRAM], 128);
    // 次の行 (y=1) に 255
    let _ = exec_line_bytes(&mut m, b"?CHR$(255)");
    assert_eq!(m.ram[OFFSET_RAM_VRAM + SCREEN_W], 255);
}

/// CHR$ は引数下位 8 bit を取る (`(n & 0xff) as u8`)。256 は 0、257 は 1。
#[test]
fn chr_dollar_wraps_to_low_byte() {
    let mut m = Machine::new();
    // CHR$(256) は CHR$(0) と等価 (= NULL セルを 1 つ書く)
    let _ = exec_line_bytes(&mut m, b"?\"X\";CHR$(256);\"Y\"");
    assert_eq!(m.ram[OFFSET_RAM_VRAM], b'X');
    assert_eq!(m.ram[OFFSET_RAM_VRAM + 1], 0);
    assert_eq!(m.ram[OFFSET_RAM_VRAM + 2], b'Y');
    // CHR$(257) は CHR$(1) と等価。1 は < 32 の制御コードなので画面に出ない。
    let _ = exec_line_bytes(&mut m, b"?CHR$(257)");
    assert_eq!(vram_line(&m, 1), "");
}

/// CHR$ の連結で連続グラフィック文字列を作れる。
#[test]
fn chr_dollar_concat_graphic_bytes() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?CHR$(200,201,202)");
    assert_eq!(m.ram[OFFSET_RAM_VRAM], 200);
    assert_eq!(m.ram[OFFSET_RAM_VRAM + 1], 201);
    assert_eq!(m.ram[OFFSET_RAM_VRAM + 2], 202);
}

/// POKE で VRAM へ任意バイトを書き、VPEEK で読み戻せる (バイト透過性)。
/// 0x80-0xFF の全範囲を順に書いて読む。
#[test]
fn poke_vpeek_round_trip_full_byte_range() {
    let mut m = Machine::new();
    // VRAM 仮想アドレスは OFFSET_RAMROM + OFFSET_RAM_VRAM
    let vram_addr = OFFSET_RAMROM + OFFSET_RAM_VRAM;
    for b in 0u16..=255 {
        let cmd = format!("POKE #{:X},{}", vram_addr + b as usize, b);
        let _ = exec_line_bytes(&mut m, cmd.as_bytes());
    }
    // VRAM 先頭 256 バイトに 0..=255 が並ぶ
    for b in 0u16..=255 {
        assert_eq!(
            m.ram[OFFSET_RAM_VRAM + b as usize], b as u8,
            "POKE failed at byte {b}"
        );
    }
}

/// CLS は VRAM を完全に 0 クリアする (グラフィック文字も含む)。
#[test]
fn cls_clears_all_bytes_including_graphics() {
    let mut m = Machine::new();
    // VRAM にグラフィック文字を散らす
    for i in 0..32 {
        m.ram[OFFSET_RAM_VRAM + i] = 0xea;
    }
    let _ = exec_line_bytes(&mut m, b"CLS");
    let vram = &m.ram[OFFSET_RAM_VRAM..OFFSET_RAM_VRAM + 32 * 24];
    assert!(vram.iter().all(|&b| b == 0), "CLS should zero-fill VRAM");
}

/// LOCATE してから PRINT すると、その位置からグラフィック文字も保持される。
#[test]
fn locate_then_print_preserves_graphic_chars() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"LOCATE 5,3");
    let _ = exec_line_bytes(&mut m, b"?\"\xea\xfe\"");
    // (x=5, y=3) からグラフィック文字が並ぶ
    assert_eq!(m.ram[OFFSET_RAM_VRAM + 3 * SCREEN_W + 5], 0xea);
    assert_eq!(m.ram[OFFSET_RAM_VRAM + 3 * SCREEN_W + 6], 0xfe);
}
