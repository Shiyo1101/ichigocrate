//! 最小限の動作確認テスト

use ichigojam_core::keycodes::{CURSOR_DOWN, CURSOR_UP, INSERT_TOGGLE};
use ichigojam_core::{exec_line, run_to_completion, Machine, OFFSET_RAM_VRAM, PC_NULL, SCREEN_W};

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
