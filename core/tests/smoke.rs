//! 最小限の動作確認テスト

use ichigojam_core::keycodes::{CURSOR_DOWN, CURSOR_UP, INSERT_TOGGLE};
use ichigojam_core::{
    exec_line, run_to_completion, BasicResult, Machine, OFFSET_RAM_VRAM, PC_NULL, SCREEN_W,
};

fn screen_text(m: &Machine) -> String {
    let mut s = String::new();
    let v = &m.ram[OFFSET_RAM_VRAM..OFFSET_RAM_VRAM + 32 * 24];
    for (i, c) in v.iter().enumerate() {
        if i > 0 && i % SCREEN_W == 0 {
            s.push('\n');
        }
        if *c == 0 {
            s.push(' ');
        } else if *c >= 32 && *c < 127 {
            s.push(*c as char);
        } else {
            s.push('?');
        }
    }
    s
}

fn vram_line(m: &Machine, y: usize) -> String {
    let v = &m.ram[OFFSET_RAM_VRAM + y * SCREEN_W..OFFSET_RAM_VRAM + (y + 1) * SCREEN_W];
    let mut s = String::new();
    for c in v {
        if *c == 0 {
            break;
        }
        s.push(*c as char);
    }
    s
}

#[test]
fn print_simple_expression() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "?1+2");
    assert_eq!(vram_line(&m, 0), "3");
}

#[test]
fn print_with_string() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "?\"HI\"");
    assert_eq!(vram_line(&m, 0), "HI");
}

#[test]
fn variable_assignment() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "A=42");
    let _ = exec_line(&mut m, "?A");
    assert!(vram_line(&m, 0).contains("42"), "got: {:?}", screen_text(&m));
}

#[test]
fn line_edit_and_list() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 ?\"X\"");
    let _ = exec_line(&mut m, "20 ?\"Y\"");
    let _ = exec_line(&mut m, "LIST");
    let t = screen_text(&m);
    assert!(t.contains("10 ?\"X\""), "LIST output missing line 10:\n{t}");
    assert!(t.contains("20 ?\"Y\""), "LIST output missing line 20:\n{t}");
}

#[test]
fn for_next_loop() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 FOR I=1 TO 3");
    let _ = exec_line(&mut m, "20 ?I");
    let _ = exec_line(&mut m, "30 NEXT");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    let t = screen_text(&m);
    assert!(t.contains("1"), "{t}");
    assert!(t.contains("2"), "{t}");
    assert!(t.contains("3"), "{t}");
}

#[test]
fn goto_and_if() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 A=5");
    let _ = exec_line(&mut m, "20 IF A=5 GOTO 40");
    let _ = exec_line(&mut m, "30 ?\"NO\"");
    let _ = exec_line(&mut m, "40 ?\"YES\"");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    let t = screen_text(&m);
    assert!(t.contains("YES"), "{t}");
    assert!(!t.contains("NO"), "{t}");
}

#[test]
fn cls_clears_vram() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "?\"HELLO\"");
    let _ = exec_line(&mut m, "CLS");
    let t = screen_text(&m);
    assert!(!t.contains("HELLO"), "{t}");
}

#[test]
fn video_modes() {
    let mut m = Machine::new();
    // 既定は通常表示 (オン・等倍・非反転)
    assert!(m.video_enabled && !m.screen_invert && m.screen_big == 0);

    // VIDEO 2: 反転、等倍
    let _ = exec_line(&mut m, "VIDEO 2");
    assert!(m.video_enabled && m.screen_invert && m.screen_big == 0);

    // VIDEO 3: 拡大 (2 倍)、非反転。論理画面は 16x12 に縮む
    let _ = exec_line(&mut m, "VIDEO 3");
    assert!(m.video_enabled && !m.screen_invert && m.screen_big == 1);
    assert_eq!((m.screen_cols(), m.screen_rows()), (16, 12));

    // VIDEO 4: 拡大反転
    let _ = exec_line(&mut m, "VIDEO 4");
    assert!(m.video_enabled && m.screen_invert && m.screen_big == 1);

    // VIDEO 1: 通常に戻る (32x24)
    let _ = exec_line(&mut m, "VIDEO 1");
    assert!(m.video_enabled && !m.screen_invert && m.screen_big == 0);
    assert_eq!((m.screen_cols(), m.screen_rows()), (32, 24));

    // VIDEO 0: 表示オフ (倍率・反転は据え置き)
    let _ = exec_line(&mut m, "VIDEO 0");
    assert!(!m.video_enabled);

    // clkdiv 引数付きでも構文エラーにならない
    let _ = exec_line(&mut m, "VIDEO 1,8");
    assert!(m.video_enabled);
}

#[test]
fn big_mode_wraps_at_reduced_width() {
    let mut m = Machine::new();
    // VIDEO 3 で 16 桁表示 (画面はクリアされる)
    let _ = exec_line(&mut m, "VIDEO 3");
    assert_eq!(m.screen_cols(), 16);
    // 対話入力相当 (挿入モード) で 18 文字打つ
    m.sync_insert_mode();
    for _ in 0..18 {
        m.screen_putc(b'X');
    }
    // 16 桁で折り返し: 1 行目は 16 文字、17/18 文字目が 2 行目へ
    let row0: String = (0..16).map(|x| m.screen_get(x, 0) as char).collect();
    assert_eq!(row0, "XXXXXXXXXXXXXXXX");
    assert_eq!(m.screen_get(0, 1) as char, 'X');
    assert_eq!(m.screen_get(1, 1) as char, 'X');
    assert_eq!(m.screen_get(2, 1), 0);
}

#[test]
fn interactive_typing_inserts() {
    let mut m = Machine::new();
    // 上書きで初期テキスト "AC" を配置 (プログラム出力相当)
    m.put_str("AC");
    // カーソルを 'C' の位置 (列 1) へ移し、対話編集 (既定=挿入) で 'B' を打つ
    m.screen_locate(1, 0);
    m.sync_insert_mode();
    m.screen_putc(b'B');
    // 挿入されて "ABC" になる ('C' が上書きされない)
    assert_eq!(vram_line(&m, 0), "ABC");
}

#[test]
fn btn_reads_key_state() {
    let mut m = Machine::new();
    // 何も押していなければ 0
    let _ = exec_line(&mut m, "?BTN(28)");
    assert_eq!(vram_line(&m, 0), "0");

    // 左矢印 (28) を押下 → 1
    m.key_set_down(28, true);
    let _ = exec_line(&mut m, "?BTN(28)");
    assert_eq!(vram_line(&m, 1), "1");

    // X キー (88) を押下 → 1。押していない右矢印 (29) は 0
    m.key_set_down(88, true);
    let _ = exec_line(&mut m, "?BTN(88)");
    let _ = exec_line(&mut m, "?BTN(29)");
    assert_eq!(vram_line(&m, 2), "1");
    assert_eq!(vram_line(&m, 3), "0");

    // 引数なし BTN() は実機ボタン → デスクトップでは常に 0
    let _ = exec_line(&mut m, "?BTN()");
    assert_eq!(vram_line(&m, 4), "0");

    // 解放したら 0 に戻る
    m.key_set_down(28, false);
    let _ = exec_line(&mut m, "?BTN(28)");
    assert_eq!(vram_line(&m, 5), "0");
}

#[test]
fn btn_negative_returns_bitmask() {
    let mut m = Machine::new();
    m.key_set_down(28, true); // 左 → bit0 (1)
    m.key_set_down(32, true); // スペース → bit4 (16)
    let _ = exec_line(&mut m, "?BTN(-1)");
    assert_eq!(vram_line(&m, 0), "17");

    // 全クリアで 0
    m.key_clear_down();
    let _ = exec_line(&mut m, "?BTN(-1)");
    assert_eq!(vram_line(&m, 1), "0");
}

#[test]
fn cursor_hidden_during_execution() {
    let mut m = Machine::new();
    m.cursorflg = true; // REPL 編集中はカーソル表示
    let _ = exec_line(&mut m, "?1");
    // コマンド/プログラム実行を始めるとカーソルは非表示になる
    assert!(!m.cursorflg);
}

#[test]
fn locate_can_show_cursor_during_execution() {
    let mut m = Machine::new();
    m.cursorflg = false;
    // プログラム側の LOCATE x,y,1 によるカーソル表示制御は引き続き可能
    let _ = exec_line(&mut m, "LOCATE 3,4,1");
    assert!(m.cursorflg);
}

#[test]
fn no_edit_or_cursor_move_during_execution() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 WAIT 600");
    let _ = exec_line(&mut m, "RUN");
    // ホスト (アプリ) は実行ループ中フラグを立てる。カーソルは非表示
    m.program_running = true;
    assert!(m.is_executing());
    assert!(!m.cursorflg);

    let before = (m.cursorx, m.cursory);
    let cell_before = m.screen_get(m.cursorx, m.cursory);
    // 実行中に矢印キー (制御コード) や文字入力を送っても無視される
    m.input_control(29); // 右矢印
    m.input_control(31); // 下矢印
    m.input_putc(b'Z');
    assert_eq!((m.cursorx, m.cursory), before, "実行中はカーソルが動かない");
    assert_eq!(
        m.screen_get(m.cursorx, m.cursory),
        cell_before,
        "実行中は文字入力で画面が書き換わらない"
    );
}

#[test]
fn input_works_after_break_even_if_pc_retained() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 WAIT 600");
    let _ = exec_line(&mut m, "RUN");
    // RUN 中: pc は LIST 内、ホストは実行中フラグを立てている
    m.program_running = true;
    assert_ne!(m.pc, PC_NULL, "RUN で pc が LIST 内に入る");

    // ESC ブレーク相当: ホストが停止を検知して実行中フラグを下ろす。
    // pc は CONT 用に保持されたまま (非 PC_NULL) になる。
    m.program_running = false;
    assert_ne!(m.pc, PC_NULL, "停止後も pc は CONT 用に保持される");

    // 回帰テスト: pc が残っていても、停止後は入力が効くこと。
    // (旧実装は is_executing を pc 基準で判定していたため入力不能だった)
    let (cx, cy) = (m.cursorx, m.cursory);
    m.input_putc(b'A');
    assert_eq!(m.screen_get(cx, cy), b'A', "停止後は入力が効く");
}

#[test]
fn editor_input_works_when_idle() {
    let mut m = Machine::new();
    assert!(!m.is_executing());
    // REPL アイドル時 (pc == PC_NULL) は対話編集が効く
    assert_eq!(m.pc, PC_NULL);
    m.input_putc(b'A');
    m.input_putc(b'B');
    assert_eq!(m.cursorx, 2);
    m.input_control(28); // 左矢印でカーソル移動できる
    assert_eq!(m.cursorx, 1);
}

#[test]
fn cursor_down_snaps_to_text_like_editor() {
    // ユーザ例:
    //   行 0: "10 LED0"
    //   行 1: "AAAA"
    let mut m = Machine::new();
    m.put_str("10 LED0\n");
    m.put_str("AAAA\n");
    m.sync_insert_mode(); // 挿入モード = テキストエディタ的カーソル移動

    // 0 行目の "0" の隣 (列 7) にカーソルを置く
    m.screen_locate(7, 0);
    assert_eq!((m.cursorx, m.cursory), (7, 0));

    // 下移動 → "AAAA" の末尾 (列 4, 行 1) へスナップ
    m.screen_putc(CURSOR_DOWN);
    assert_eq!((m.cursorx, m.cursory), (4, 1));

    // さらに下移動 → 空行なので 0 列 (行 2) へ
    m.screen_putc(CURSOR_DOWN);
    assert_eq!((m.cursorx, m.cursory), (0, 2));
}

#[test]
fn cursor_up_snaps_to_text_end() {
    let mut m = Machine::new();
    m.put_str("AB\n"); // 行 0: "AB"
    m.put_str("CDEFG\n"); // 行 1: "CDEFG"
    m.sync_insert_mode();

    // "CDEFG" の末尾 (列 5, 行 1)
    m.screen_locate(5, 1);
    m.screen_putc(CURSOR_UP);
    // 上の行の短いテキスト "AB" の末尾 (列 2, 行 0) へスナップ
    assert_eq!((m.cursorx, m.cursory), (2, 0));
}

#[test]
fn cursor_free_move_in_overwrite_mode() {
    let mut m = Machine::new();
    m.put_str("AB\n");
    m.put_str("CDEFG\n");
    // 挿入/上書きトグルで上書きモードへ
    m.screen_putc(INSERT_TOGGLE);
    m.sync_insert_mode();

    m.screen_locate(5, 1);
    m.screen_putc(CURSOR_UP);
    // 上書きモードはスナップせず自由移動 (実機準拠)
    assert_eq!((m.cursorx, m.cursory), (5, 0));
}

#[test]
fn cursor_width_follows_edit_mode() {
    let mut m = Machine::new();
    // 既定の挿入モードはカーソルが左半分 (細い)
    m.sync_insert_mode();
    assert!(!m.cursor_full_width(), "挿入モードは左半分カーソル");

    // 挿入/上書きトグルで上書きモードへ → カーソルは全幅
    m.screen_putc(INSERT_TOGGLE);
    m.sync_insert_mode();
    assert!(m.cursor_full_width(), "上書きモードは全幅カーソル");
}

#[test]
fn hex_and_bin() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "?#ff");
    let _ = exec_line(&mut m, "?`101");
    let t = screen_text(&m);
    assert!(t.contains("255"), "{t}");
    assert!(t.contains("5"), "{t}");
}

#[test]
fn kbd_sets_keyboard_id_and_ver_2_reflects() {
    // KBD コマンド (Ver1.5) はキーボードレイアウト ID を 0/1 に切り替える。
    // 引数は !!n で正規化される (実機 IchigoJam_P/src/keyboard.h:34 と同様)。
    // VER(2) が現在の ID を返すこと。
    let mut m = Machine::new();

    // 初期値は 0 (US)
    let _ = exec_line(&mut m, "?VER(2)");
    assert_eq!(vram_line(&m, 0), "0");

    // KBD 1 → JA。?VER(2) の改行でカーソルは y=1 へ進むだけ
    // (exec_line は OK を出さない) なので、2 回目の VER(2) は y=1 に書かれる。
    let _ = exec_line(&mut m, "KBD 1");
    assert_eq!(m.keyboard_id(), 1);
    let _ = exec_line(&mut m, "?VER(2)");
    assert_eq!(vram_line(&m, 1), "1");

    // KBD 0 → US
    let _ = exec_line(&mut m, "KBD 0");
    assert_eq!(m.keyboard_id(), 0);

    // KBD 2 のような 0 以外の任意値は JA (1) に正規化される
    let _ = exec_line(&mut m, "KBD 2");
    assert_eq!(m.keyboard_id(), 1);

    // 負の値も 0 以外なので JA 扱い
    let _ = exec_line(&mut m, "KBD -5");
    assert_eq!(m.keyboard_id(), 1);
}

#[test]
fn kbd_switches_physical_key_translation() {
    // KBD コマンドで keymap_lookup の引く表が US/JA で実際に切替わること。
    // HID 0x1f は数字 2 の物理キー。日本語配列で Shift+2 を打ったとき、
    // KBD 0 なら US 解釈で '@'、KBD 1 なら JA 解釈で '"' が返る。
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "KBD 0");
    assert_eq!(m.keymap_lookup(0x1f, true, false), b'@');
    let _ = exec_line(&mut m, "KBD 1");
    assert_eq!(m.keymap_lookup(0x1f, true, false), b'"');
    // 0x2f: US `[` / JA `@`
    let _ = exec_line(&mut m, "KBD 0");
    assert_eq!(m.keymap_lookup(0x2f, false, false), b'[');
    let _ = exec_line(&mut m, "KBD 1");
    assert_eq!(m.keymap_lookup(0x2f, false, false), b'@');
}

// ============================================================
// グラフィック文字 (128-255) のバイト保持テスト
//
// バグ報告: プログラム記述後に実行すると別の文字に変わる。
// 原因: REPL 経路で String::push(c as char) → as_bytes() の UTF-8 化により
// 128-255 のバイトが 2 バイトに展開されてしまう。
// 直接モード (RAM への直接書き込み) ではこのバグは出ない。
// ============================================================

use ichigojam_core::{exec_line_bytes, OFFSET_RAM_LINEBUF};

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
    let list = &m.ram[ichigojam_core::OFFSET_RAM_LIST..];
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

// ============================================================
// バイト境界・特殊コードの網羅
// ============================================================

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

/// POKE で VRAM へ任意バイトを書き、VPEEK で読み戻せる (バイト透過性)。
/// 0x80-0xFF の全範囲を順に書いて読む。
#[test]
fn poke_vpeek_round_trip_full_byte_range() {
    let mut m = Machine::new();
    // VRAM 仮想アドレスは OFFSET_RAMROM + OFFSET_RAM_VRAM
    let vram_addr = ichigojam_core::OFFSET_RAMROM + OFFSET_RAM_VRAM;
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

/// PRINT のセミコロン (`;`) は末尾改行を抑制する。
#[test]
fn print_semicolon_suppresses_newline() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?\"A\";");
    let _ = exec_line_bytes(&mut m, b"?\"B\"");
    // 改行抑制なので "A" と "B" が連結して y=0 に "AB"
    assert_eq!(vram_line(&m, 0), "AB");
}

/// PRINT のカンマ (`,`) は値の間にスペース 1 個を入れる。
#[test]
fn print_comma_inserts_space() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?1,2,3");
    assert_eq!(vram_line(&m, 0), "1 2 3");
}

/// 空入力行は何もしない (パニックしないこと)。
#[test]
fn empty_line_is_noop() {
    let mut m = Machine::new();
    let r = exec_line_bytes(&mut m, b"");
    assert!(r.is_ok());
}

/// プログラム無しで RUN しても OK (パニックしない)。
#[test]
fn run_with_no_program_is_safe() {
    let mut m = Machine::new();
    assert_eq!(m.listsize, 0);
    let r = exec_line_bytes(&mut m, b"RUN");
    assert!(r.is_ok());
    assert_eq!(m.pc, PC_NULL, "空プログラムの RUN 後は pc が NULL");
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

/// HEX$ で 0xFF を 2 桁指定で出力すると "FF" になる。
#[test]
fn hex_dollar_two_digits() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?HEX$(255,2)");
    assert_eq!(vram_line(&m, 0), "FF");
}

/// BIN$ で 0b1010 を 4 桁指定で出力すると "1010" になる。
#[test]
fn bin_dollar_four_digits() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?BIN$(10,4)");
    assert_eq!(vram_line(&m, 0), "1010");
}

/// DEC$ で負数の桁指定 (右寄せ) が効くこと。
#[test]
fn dec_dollar_right_justifies() {
    let mut m = Machine::new();
    let _ = exec_line_bytes(&mut m, b"?DEC$(7,3)");
    // 3 桁右寄せ: "  7"
    assert_eq!(vram_line(&m, 0), "  7");
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

// ============================================================
// 未テストだったコマンド群の C 実装との突合せテスト
// ============================================================

use ichigojam_core::ram::{OFFSET_RAM_PCG, SIZE_RAM_PCG};
use ichigojam_core::OFFSET_RAM_LIST;

/// NEW: LIST 領域がゼロクリアされ listsize/pc/pcbreak も初期化される
/// (basic.h:2203 command_new と同等)。
#[test]
fn new_clears_list_and_pc() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 ?\"X\"");
    let _ = exec_line(&mut m, "20 ?\"Y\"");
    assert!(m.listsize > 0);
    let _ = exec_line(&mut m, "NEW");
    assert_eq!(m.listsize, 0);
    assert_eq!(m.pc, PC_NULL);
    // LIST 先頭バイトが全部 0 になっていることをサンプリング
    for &b in &m.ram[OFFSET_RAM_LIST..OFFSET_RAM_LIST + 32] {
        assert_eq!(b, 0);
    }
}

/// END / STOP: PC を NULL に戻す (basic.h:2544)
#[test]
fn end_resets_pc() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 END");
    let _ = exec_line(&mut m, "20 ?\"never\"");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    assert_eq!(m.pc, PC_NULL);
    // 20 行に到達していないこと
    assert!(!screen_text(&m).contains("never"));
}

/// CLV: 変数領域 (配列 0..101 + A-Z) がゼロクリアされる (basic.h:2871)。
/// `A` は内部インデックス `IJB_SIZEOF_ARRAY (=102)` から始まる。
#[test]
fn clv_clears_variables() {
    use ichigojam_core::ram::IJB_SIZEOF_ARRAY;
    let var_a = IJB_SIZEOF_ARRAY;
    let var_b = IJB_SIZEOF_ARRAY + 1;
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "A=123");
    let _ = exec_line(&mut m, "B=456");
    let _ = exec_line(&mut m, "[3]=7"); // 配列要素も書く
    assert_eq!(m.var_get(var_a), 123);
    assert_eq!(m.var_get(var_b), 456);
    assert_eq!(m.var_get(3), 7);
    let _ = exec_line(&mut m, "CLV");
    assert_eq!(m.var_get(var_a), 0);
    assert_eq!(m.var_get(var_b), 0);
    assert_eq!(m.var_get(3), 0);
}

/// CLK: キーバッファを空にする (basic.h:3117)。
/// 直後の `key_get_key` は -1 (未取得) を返す。
#[test]
fn clk_clears_key_buffer() {
    let mut m = Machine::new();
    m.key_push(b'X');
    m.key_push(b'Y');
    let _ = exec_line(&mut m, "CLK");
    assert_eq!(m.key_get_key(), -1);
}

/// CLT: tick カウンタ (frames) をゼロに戻す (basic.h:2866)
#[test]
fn clt_resets_frames_counter() {
    let mut m = Machine::new();
    m.frames = 1234;
    let _ = exec_line(&mut m, "CLT");
    assert_eq!(m.frames, 0);
}

/// CLP: PCG (書き換え可能キャラクタ) をフォント末尾 32 文字で初期化する
/// (basic.h:3113 → screen_clp)
#[test]
fn clp_resets_pcg_to_font_tail() {
    let mut m = Machine::new();
    // まず PCG を全部 0xFF で潰す
    m.ram[OFFSET_RAM_PCG..OFFSET_RAM_PCG + SIZE_RAM_PCG].fill(0xff);
    let _ = exec_line(&mut m, "CLP");
    // CLP 後は font の末尾 32 文字でちょうど埋まる
    // (内容は font 依存のためフィールド全部が 0xff のままではないことを確認)
    let pcg = &m.ram[OFFSET_RAM_PCG..OFFSET_RAM_PCG + SIZE_RAM_PCG];
    assert!(pcg.iter().any(|&b| b != 0xff), "PCG should be reset");
}

/// LED: 引数 0 で消灯、それ以外で点灯 (basic.h:2812)
#[test]
fn led_command_toggles_led_state() {
    let mut m = Machine::new();
    assert!(!m.led);
    let _ = exec_line(&mut m, "LED 1");
    assert!(m.led);
    let _ = exec_line(&mut m, "LED 0");
    assert!(!m.led);
    // 0 以外は ON
    let _ = exec_line(&mut m, "LED 42");
    assert!(m.led);
}

/// SRND: 同じシードを与えれば RND は同じ値を返す (basic.h:3144)
#[test]
fn srnd_makes_rnd_reproducible() {
    let mut a = Machine::new();
    let mut b = Machine::new();
    let _ = exec_line(&mut a, "SRND 42");
    let _ = exec_line(&mut b, "SRND 42");
    let _ = exec_line(&mut a, "?RND(100)");
    let _ = exec_line(&mut b, "?RND(100)");
    assert_eq!(vram_line(&a, 0), vram_line(&b, 0));
    // 異なるシードならふつう違う値になる
    let mut c = Machine::new();
    let _ = exec_line(&mut c, "SRND 1");
    let _ = exec_line(&mut c, "?RND(10000)");
    let _ = exec_line(&mut b, "?RND(10000)");
    // 完全一致になる可能性はゼロではないが、IchigoJam の xorshift では実質起きない
    assert_ne!(vram_line(&c, 0), vram_line(&b, 1));
}

/// SCROLL 0/1/2/3 がそれぞれ UP/RIGHT/DOWN/LEFT に対応 (screen.h:309)
#[test]
fn scroll_directions_move_vram() {
    // SCROLL 0 (UP): 2 行目に書いた文字が 1 行目に来る
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "LOCATE 0,1");
    let _ = exec_line(&mut m, "?\"AB\";");
    let _ = exec_line(&mut m, "SCROLL 0");
    assert_eq!(vram_line(&m, 0), "AB");

    // SCROLL 1 (RIGHT): 0 列目に書いた文字が 1 列目に来る
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "LOCATE 0,0");
    let _ = exec_line(&mut m, "?\"P\";");
    let _ = exec_line(&mut m, "SCROLL 1");
    assert_eq!(m.ram[OFFSET_RAM_VRAM + 1], b'P');
    assert_eq!(m.ram[OFFSET_RAM_VRAM], 0);

    // SCROLL 2 (DOWN): 0 行目に書いた文字が 1 行目に来る
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "LOCATE 0,0");
    let _ = exec_line(&mut m, "?\"CD\";");
    let _ = exec_line(&mut m, "SCROLL 2");
    assert_eq!(vram_line(&m, 1), "CD");

    // SCROLL 3 (LEFT): 1 列目に書いた文字が 0 列目に来る
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "LOCATE 1,0");
    let _ = exec_line(&mut m, "?\"Q\";");
    let _ = exec_line(&mut m, "SCROLL 3");
    assert_eq!(m.ram[OFFSET_RAM_VRAM], b'Q');
}

/// COPY: 正の len は順方向、負の len は逆方向 (basic.h:3082)。
/// COPY のアドレスは仮想アドレス (OFFSET_RAMROM 加算済み) を渡すため、
/// PCG 先頭 (ram[0]) は仮想アドレス OFFSET_RAMROM になる。
#[test]
fn copy_forward_and_backward() {
    use ichigojam_core::OFFSET_RAMROM;
    let pcg_v = OFFSET_RAMROM as i32; // PCG 先頭の仮想アドレス
    let mut m = Machine::new();
    // src 範囲 (PCG[0..8]) に "ABCDEFGH" を直接書く
    for i in 0..8 {
        m.ram[OFFSET_RAM_PCG + i] = b'A' + i as u8;
    }
    // 順方向: dst=PCG+8, src=PCG+0, len=8 → PCG[8..16] が "ABCDEFGH"
    let cmd = format!("COPY {},{},8", pcg_v + 8, pcg_v);
    let _ = exec_line(&mut m, &cmd);
    for i in 0..8 {
        assert_eq!(m.ram[OFFSET_RAM_PCG + 8 + i], b'A' + i as u8);
    }

    // 逆方向: 重なりがある領域を 1 byte 後ろへシフト。
    // dst=PCG+7, src=PCG+6, len=-7 → ram[7]←ram[6], ram[6]←ram[5], ...
    //                                ram[1]←ram[0]。先頭 ram[0] は変わらない。
    for i in 0..8 {
        m.ram[OFFSET_RAM_PCG + i] = b'A' + i as u8;
    }
    let cmd = format!("COPY {},{},-7", pcg_v + 7, pcg_v + 6);
    let _ = exec_line(&mut m, &cmd);
    assert_eq!(m.ram[OFFSET_RAM_PCG + 7], b'G');
    assert_eq!(m.ram[OFFSET_RAM_PCG + 6], b'F');
    assert_eq!(m.ram[OFFSET_RAM_PCG + 1], b'A');
    assert_eq!(m.ram[OFFSET_RAM_PCG], b'A');
}

/// `@LABEL` を文として書くと、`:` または行末までコメント扱いになる
/// (basic.h:3156 command_at, 1.2b40 仕様)
#[test]
fn at_label_statement_is_comment() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 @TOP:?\"HIT\"");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    // `:` 以降は実行されるので "HIT" が出る
    assert!(screen_text(&m).contains("HIT"));
}

/// `GOTO @LABEL` は LIST から `@LABEL` 行を探してその行へ飛ぶ
/// (basic.h:1573 token_expression5 内の TOKEN_AT 処理)
#[test]
fn goto_at_label_jumps_to_label_line() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 GOTO @SKIP");
    let _ = exec_line(&mut m, "20 ?\"NO\"");
    let _ = exec_line(&mut m, "30 @SKIP");
    let _ = exec_line(&mut m, "40 ?\"YES\"");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    assert!(screen_text(&m).contains("YES"));
    assert!(!screen_text(&m).contains("NO"));
}

/// INPUT は即値の代入はせず、プロンプトを出して入力待ちに入る。
/// 確定するまで変数は元の値のまま (代入は input_complete が担う)。
#[test]
fn input_enters_await_state_without_assigning() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "A=99");
    let _ = exec_line(&mut m, "INPUT A");
    assert!(m.is_awaiting_input());
    assert_eq!(m.var_get(102), 99);
}

/// BEEP (引数なし): 既定 TONE=10, LEN=3。tone>0 で current_tone_hz が
/// 0 でない値になる (basic.h:2963)。
#[test]
fn beep_no_args_uses_default_tone() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "BEEP");
    assert!(m.psg_sound());
    assert!(m.current_tone_hz > 0.0);
}

/// PLAY (引数なし): MML 停止 (basic.h:2934)。psgmml が None になり
/// 無音になる。
#[test]
fn play_with_no_arg_stops_mml() {
    let mut m = Machine::new();
    // 一度 BEEP で鳴らしてから PLAY (引数なし) で止める
    let _ = exec_line(&mut m, "BEEP 10,1000");
    assert!(m.psg_sound());
    let _ = exec_line(&mut m, "PLAY");
    assert!(!m.psg_sound());
}

/// RENUM: 行番号を再採番する (basic.h:2481)。GOTO/GOSUB を含まない
/// プレーン行で行番号自体が振り直されること。参照書換は別テスト
/// (`renum_rewrites_goto_and_gosub_references`) で検証する。
#[test]
fn renum_renumbers_line_numbers_only() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "5 ?\"A\"");
    let _ = exec_line(&mut m, "7 ?\"B\"");
    let _ = exec_line(&mut m, "99 ?\"C\"");
    let _ = exec_line(&mut m, "RENUM 10,10");
    // 行番号が 10, 20, 30 に振り直される
    assert_eq!(m.list_get_number(0), 10);
    let mut idx = m.list_get_length(0) as u16 + 4;
    assert_eq!(m.list_get_number(idx), 20);
    idx += m.list_get_length(idx) as u16 + 4;
    assert_eq!(m.list_get_number(idx), 30);
}

/// RENUM が GOTO/GOSUB の数値リテラル参照も書き換える (basic.h:2389
/// command_renum2 と同等)。
///
/// 桁数を変えない範囲 (2 桁→2 桁) で正しさを確認し、続けて 3 桁→1 桁
/// (縮小方向は本移植でも常に成功) のケースも検証する。
#[test]
fn renum_rewrites_goto_and_gosub_references() {
    // === 2 桁→2 桁 ===
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 GOTO 30");
    let _ = exec_line(&mut m, "20 ?\"MID\"");
    let _ = exec_line(&mut m, "30 GOSUB 20");
    let _ = exec_line(&mut m, "40 GOTO 10");
    let _ = exec_line(&mut m, "RENUM 50,10");
    // 行番号: 10,20,30,40 → 50,60,70,80
    assert_eq!(m.list_get_number(0), 50);
    // 参照: GOTO 30 → GOTO 70、GOSUB 20 → GOSUB 60、GOTO 10 → GOTO 50
    // (走らせて MID が出ることを 1 サイクル分で確認する。RUN 後に
    // GOSUB→RETURN→次行 GOTO 50 で 50 行目に戻ってループするため、
    // basic_step を有限回ぶん回して MID が出るかを見る。)
    let _ = exec_line(&mut m, "RUN");
    for _ in 0..500 {
        if m.basic_step().is_some() {
            break;
        }
        if screen_text(&m).contains("MID") {
            break;
        }
    }
    assert!(
        screen_text(&m).contains("MID"),
        "GOSUB 60 が 60 行目に届いていない: {:?}",
        screen_text(&m)
    );

    // === 3 桁→1 桁 (縮小) ===
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "100 GOTO 300");
    let _ = exec_line(&mut m, "200 ?\"OK\"");
    let _ = exec_line(&mut m, "300 GOSUB 200");
    let _ = exec_line(&mut m, "RENUM 1,1");
    assert_eq!(m.list_get_number(0), 1);
    let _ = exec_line(&mut m, "RUN");
    for _ in 0..200 {
        if m.basic_step().is_some() {
            break;
        }
        if screen_text(&m).contains("OK") {
            break;
        }
    }
    assert!(
        screen_text(&m).contains("OK"),
        "GOSUB 200→2 への縮小書換が機能していない: {:?}",
        screen_text(&m)
    );
}

/// RENUM が桁数オーバーで Illegal argument を返す: 1 桁参照 (例: `GOTO 5`) を
/// 3 桁の新番号に振り直すと行内バッファに収まらないため拒否する
/// (C 実装の align 1byte シフト相当は本移植では未対応)。
#[test]
fn renum_rejects_digit_overflow() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "5 GOTO 5");
    let res = exec_line(&mut m, "RENUM 100,100");
    assert!(res.is_err(), "1 桁→3 桁の参照書換は拒否されるべき");
}

/// RENUM の引数バリデーション: start<=0 または step<=0 は Illegal argument
/// (basic.h:2487)。
#[test]
fn renum_rejects_non_positive_args() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 ?\"X\"");
    let res = exec_line(&mut m, "RENUM 0,10");
    assert!(res.is_err());
    let res = exec_line(&mut m, "RENUM 10,0");
    assert!(res.is_err());
}

/// OK 2: noresmode (応答抑制) ON、それ以外は OFF (commands.rs:331)
#[test]
fn ok_2_enables_quiet_mode() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "OK 2");
    // noresmode は pub(crate) のため公開 API では直接見えない。
    // 次の `OK 0` で OFF に戻すことだけ通す動作確認とする。
    let _ = exec_line(&mut m, "OK 0");
}

/// CONT: ESC ブレーク後、停止行から再開する (basic.h:1888)。
/// STOP/END は `pcbreak` も NULL にするため CONT で再開できないのが仕様。
/// CONT が効くのは「ESC でブレークしたとき」だけ。
#[test]
fn cont_resumes_from_esc_break() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 A=A+1");
    let _ = exec_line(&mut m, "20 IF A<100 GOTO 10");
    let _ = exec_line(&mut m, "30 ?\"DONE\"");
    let _ = exec_line(&mut m, "RUN");
    // 数ステップ実行したところで ESC を入れて中断させる
    for _ in 0..5 {
        if m.basic_step().is_some() {
            break;
        }
    }
    m.key_flg_esc = 1;
    while m.pc != PC_NULL {
        if m.basic_step().is_some() {
            break;
        }
    }
    m.key_flg_esc = 0;
    // ESC ブレーク後は pcbreak に位置が記録され、CONT で再開できる。
    // ループ条件 A<100 が成立する限り 10→20 を繰り返し、最後に 30 で DONE。
    let _ = exec_line(&mut m, "CONT");
    run_to_completion(&mut m);
    assert!(
        screen_text(&m).contains("DONE"),
        "CONT should resume the loop until A>=100, then print DONE"
    );
}

/// STOP は pcbreak を NULL に戻すため、その後の CONT は無効化される
/// (basic.h:2544 / 2546 で `pc = pcbreak = NULL`)。
#[test]
fn stop_disables_subsequent_cont() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 STOP");
    let _ = exec_line(&mut m, "20 ?\"AFTER\"");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    assert_eq!(m.pc, PC_NULL);
    let _ = exec_line(&mut m, "CONT");
    run_to_completion(&mut m);
    // STOP は pcbreak=NULL にするので CONT しても 20 行へは進まない
    assert!(!screen_text(&m).contains("AFTER"));
}

/// プログラム実行中の INPUT は basic_step が `BasicResult::Input` を返して
/// 入力待ちに入り、`input_complete` で受け取った値を変数へ代入して実行を
/// 再開する (basic.h:2136 command_input / IJB_DONT_LOOP)。
#[test]
fn input_assigns_typed_value_during_run() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 INPUT A");
    let _ = exec_line(&mut m, "20 ?A*2");
    let _ = exec_line(&mut m, "RUN");

    // INPUT 文に達すると Input が返り、入力待ちになる。
    let mut hit = false;
    for _ in 0..50 {
        if let Some(r) = m.basic_step() {
            assert_eq!(r, BasicResult::Input);
            hit = true;
            break;
        }
    }
    assert!(hit, "INPUT が入力待ち (BasicResult::Input) を返さなかった");
    assert!(m.is_awaiting_input());
    // デフォルトプロンプト '?' が表示されている。
    assert!(screen_text(&m).contains('?'));

    m.input_complete(b"21");
    assert!(!m.is_awaiting_input());
    assert_eq!(m.var_get(102), 21);

    // 入力後は INPUT 文の直後 (?A*2) から継続して 42 を出力する。
    run_to_completion(&mut m);
    assert!(
        screen_text(&m).contains("42"),
        "INPUT 後に実行が継続していない: {:?}",
        screen_text(&m)
    );
}

/// INPUT は数値リテラルだけでなく式も入力値として評価する (C: command_let2)。
/// 文字列プロンプト付き構文 `INPUT "...",var` も確認する。
#[test]
fn input_evaluates_expression_with_prompt() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "B=3");
    // 即時モードの INPUT は AwaitingInput を返す。
    let _ = exec_line(&mut m, "INPUT \"VAL\",A");
    assert!(m.is_awaiting_input());
    assert!(screen_text(&m).contains("VAL"));

    m.input_complete(b"B+4");
    assert_eq!(m.var_get(102), 7);
}

/// 空入力 (Enter のみ) はパースに失敗するため、C の errorignore と同様に
/// 代入をスキップし、変数は元の値のまま残す。
#[test]
fn input_empty_keeps_previous_value() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "A=5");
    let _ = exec_line(&mut m, "INPUT A");
    assert!(m.is_awaiting_input());

    m.input_complete(b"");
    assert!(!m.is_awaiting_input());
    assert_eq!(m.var_get(102), 5);
}

/// cancel_input は代入せず入力待ちを解除する (ESC 中断相当)。
#[test]
fn cancel_input_clears_pending_without_assigning() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "A=9");
    let _ = exec_line(&mut m, "INPUT A");
    assert!(m.is_awaiting_input());

    m.cancel_input();
    assert!(!m.is_awaiting_input());
    assert_eq!(m.var_get(102), 9);
}
