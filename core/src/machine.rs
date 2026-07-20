// Copyright 2014-2024 the IchigoJam authors. All rights reserved. MIT license.
// 移植元:
// https://github.com/IchigoJam/ichigojam-firm/blob/main/IchigoJam_BASIC/vars.h
// https://github.com/IchigoJam/ichigojam-firm/blob/main/IchigoJam_BASIC/random.h
// https://github.com/IchigoJam/ichigojam-firm/blob/main/IchigoJam_BASIC/basic.h

//! IchigoJam 仮想マシンの中核状態。
//!
//! 元実装はグローバル変数の集合だが、本移植では `Machine` 構造体に集約し
//! `&mut self` 経由で操作する。

use std::collections::VecDeque;

use crate::errors::BasicError;
use crate::font::CHAR_PATTERN_JP;
use crate::ram::*;

pub const PC_NULL: usize = usize::MAX;

/// 起動時に表示する "IchigoCrate BASIC ..." バナー (実機の `IJB_TITLE` 相当だが、
/// 商標である "IchigoJam" ではなく本プロジェクト名 "IchigoCrate" を用い、
/// バージョンも本プロジェクト独自の 1.0 から始める)。
/// `web`/`app` の両ホストで同一文言が重複していたため、`power_on_reset` に
/// 一本化した。
const BOOT_BANNER: &str = "IchigoCrate BASIC 1.0\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasicResult {
    StopOrErr,
    /// 正常終了 (呼び出し側は `OK` を表示する)
    Execute,
    /// 行番号付き入力により LIST が編集された (`OK` は表示しない)
    Edit,
    /// `INPUT` 文がプロンプトを表示し、対話入力待ちに入った。ホストは 1 行
    /// 入力させたあと [`Machine::input_complete`] を呼び、実行を再開させる。
    Input,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Token {
    pub code: u16,
    pub value: i16,
}

/// IchigoJam BASIC 仮想マシン。
///
/// 公開フィールドはホスト (UI / テスト) が直接参照する必要のあるものだけ。
/// それ以外はクレート内専用 (`pub(crate)`) で、外部からはアクセサ経由で
/// 触る。移植元の名残で getter を介さず直アクセスしているフィールドが
/// 多いため、内部用にはまだ全部 `pub(crate)` で公開している。
#[derive(Debug)]
pub struct Machine {
    /// 統合 RAM (PCG/VAR/VRAM/LIST/KEYBUF/LINEBUF/I2CBUF)
    pub ram: Vec<u8>,
    /// プログラムカウンタ (ram のインデックス)。PC_NULL なら未走行。
    pub pc: usize,
    /// LIST の使用バイト数 (末尾 0x00 0x00 を除く)
    pub list_size: u16,

    // ===== スクリーン状態 (ホストが描画時に読む) =====
    pub cursorx: i32,
    pub cursory: i32,
    pub is_cursor_visible: bool,
    pub is_screen_inverted: bool,
    /// 拡大表示の段階 (VIDEO 3/4 で 1 以上)。表示倍率は `1 << screen_zoom_shift`。
    /// 0 = 等倍, 1 = 2 倍, 2 = 4 倍, 3 = 8 倍 (最大 3 でクリップ)。
    pub screen_zoom_shift: u8,
    /// 映像出力の有効/無効 (VIDEO 0 でオフ)。ホストはオフ時に黒画面を描画する。
    pub is_video_enabled: bool,

    // ===== キーボード関連 (ホストが書く) =====
    pub is_kana_mode: bool,
    /// ESC による中断要求が立っているか。実行ループの停止判定に使う。
    pub is_esc_pressed: bool,

    // ===== タイマ (ホストが 60Hz で更新) =====
    pub frames: u16,

    // ===== I/O 状態 (ホストが LED 枠線色決定で読む) =====
    pub is_led_on: bool,

    // ===== サウンド出力 (UI/Audio スレッド連携) =====
    /// 現在の周波数 (Hz)。0 なら無音。
    pub current_tone_hz: f32,

    // ===== WAIT 用フレームカウンタ (協調的待機) =====
    /// 残り待機フレーム数。0 でなければ basic_step は即 return する。
    pub wait_frames: u32,

    // ---- 以下はクレート内専用 (`pub(crate)`) ----

    /// `token_back` 用に直前のトークン取得開始位置を覚える
    pub(crate) last_token_start_pc: usize,
    pub(crate) last_token_end_pc: usize,
    pub(crate) last_token: Token,
    pub(crate) break_resume_pc: usize,
    /// 直近の停止理由。実行ループ ([`Machine::basic_step`]) が `Err` を捕捉して
    /// 格納し、境界 (`exec_line`) が読み取る。
    pub(crate) last_error: Option<BasicError>,
    pub(crate) gosub_depth: u8,
    pub(crate) for_depth: u8,
    pub(crate) gosub_stack: [usize; IJB_SIZEOF_GOSUB_STACK],
    pub(crate) for_stack: [usize; IJB_SIZEOF_FOR_STACK],
    /// トークン解釈が式の文脈にあるか (false=コマンド, true=式)。
    pub(crate) is_expr_mode: bool,

    pub(crate) text_cols: usize,
    pub(crate) text_rows: usize,
    /// 文字描画が上書きモードか (false=挿入, true=上書き)。
    pub(crate) is_overwrite_mode: bool,
    pub(crate) locate_pending_bytes: u8,

    /// Insert キーのトグル状態 (false=挿入, true=上書き)。[`Machine::is_overwrite_mode`]
    /// の同期元。
    pub(crate) is_overwrite_toggle: bool,
    /// ローマ字かな変換の未確定バッファ (子音 1〜2 文字目)
    pub(crate) romaji_pending_0: u8,
    pub(crate) romaji_pending_1: u8,
    /// INKEY() 用のキューイング入力バッファ
    pub(crate) inkey_queue: VecDeque<u8>,
    /// キーボードレイアウト ID (`KBD` コマンド / `VER(2)` 用)。
    /// 0 = US, 1 = JA。デフォルトは JA。`KBD n` は `!!n` で正規化される。
    /// 実機はフラッシュへ永続化するが本移植はメモリ内のみ。
    pub(crate) keyboard_id: u8,
    /// 現在押下中のキー (BTN() 用)。ASCII コードで索引する押下フラグ。
    /// ホストがキー押下/解放ごとに [`Machine::key_set_down`] で更新する。
    pub(crate) keys_down: [bool; 256],
    /// プログラムを継続実行中か。ホスト (アプリ) が毎フレーム自身の実行
    /// ループ状態と同期させる。RUN 中は true、END/STOP/ESC ブレーク/完了で
    /// false。`pc` は STOP/ブレーク後も CONT 用に保持されるため実行中判定には
    /// 使えない。対話編集の入力可否はこのフラグで判断する。
    pub is_program_running: bool,
    pub(crate) is_quiet_mode: bool,

    /// `INPUT` 文の入力待ち状態。`Some(target)` のとき、ホストが 1 行入力を
    /// 受け取って [`Machine::input_complete`] を呼ぶまで実行を中断する。
    /// `target` は代入先の変数/配列要素のスロット番号。
    pub(crate) input_pending: Option<usize>,

    pub(crate) psg_octave: u8,
    /// デフォルト音長 (MML `L` で変更)
    pub(crate) psg_default_note_32nds: u8,
    pub(crate) is_tone_active: bool,
    pub(crate) psg_tempo_bpm: u16,
    /// 現在の音・休符の残りフレーム数 (0 で次のノートへ進む)
    pub(crate) psg_remaining_frames: u32,
    /// MML 文字列の RAM インデックス (None = 演奏終了)
    pub(crate) psg_mml_pos: Option<usize>,
    /// MML `$` のリピート開始位置
    pub(crate) psg_mml_repeat_pos: Option<usize>,

    /// xorshift 乱数の内部状態
    pub(crate) rnd_state: [u32; 4],

    /// 最後に SAVE/LOAD した slot 番号 (FILE() で参照)
    pub(crate) last_file_slot: u8,

    /// ホスト側ストレージ (デスクトップではディスク)。None なら File error。
    pub(crate) storage: Option<Box<dyn Storage>>,
}

/// SAVE/LOAD/FILES のホスト側実装。デスクトップアプリは実ファイル、
/// テスト/組込はメモリ実装を差し込む。
pub trait Storage: std::fmt::Debug {
    /// 指定スロットへ data 全体を保存。成功なら `true`。
    fn save(&mut self, slot: u8, data: &[u8]) -> bool;
    /// 指定スロットから最大 `buf.len()` バイトを読み出す。
    /// 読込んだバイト数 (`Some`) または失敗 (`None`) を返す。
    /// `None` の場合の `buf` の中身は不定。
    fn load(&mut self, slot: u8, buf: &mut [u8]) -> Option<usize>;
    /// FILES 用: スロット先頭バイトの覗き見。意味は [`Storage::load`] と同じ。
    fn peek(&mut self, slot: u8, buf: &mut [u8]) -> Option<usize>;
    /// 利用可能なスロット数 (FILES デフォルト範囲)。
    fn slot_count(&self) -> u8 {
        16
    }
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    pub fn new() -> Self {
        let mut m = Self {
            ram: vec![0u8; SIZE_RAM],
            pc: PC_NULL,
            last_token_start_pc: 0,
            last_token_end_pc: 0,
            last_token: Token::default(),
            break_resume_pc: PC_NULL,
            last_error: None,
            gosub_depth: 0,
            for_depth: 0,
            gosub_stack: [0; IJB_SIZEOF_GOSUB_STACK],
            for_stack: [0; IJB_SIZEOF_FOR_STACK],
            is_expr_mode: false,
            list_size: 0,

            cursorx: 0,
            cursory: 0,
            text_cols: SCREEN_W,
            text_rows: SCREEN_H,
            is_cursor_visible: true,
            is_overwrite_mode: true,
            locate_pending_bytes: 0,
            is_screen_inverted: false,
            screen_zoom_shift: 0,
            is_video_enabled: true,

            is_overwrite_toggle: false,
            is_kana_mode: false,
            romaji_pending_0: 0,
            romaji_pending_1: 0,
            is_esc_pressed: false,
            inkey_queue: VecDeque::with_capacity(128),
            keyboard_id: 1,
            keys_down: [false; 256],
            is_program_running: false,
            is_quiet_mode: false,
            input_pending: None,

            psg_octave: 3,
            psg_default_note_32nds: 8,
            is_tone_active: false,
            psg_tempo_bpm: 120,
            psg_remaining_frames: 0,
            psg_mml_pos: None,
            psg_mml_repeat_pos: None,

            frames: 0,
            rnd_state: [123456789, 362436069, 521288629, 88675123],

            is_led_on: false,
            last_file_slot: 0,

            current_tone_hz: 0.0,
            wait_frames: 0,
            storage: None,
        };
        m.basic_init();
        m
    }

    /// ストレージ実装を差し込む (デスクトップアプリは DiskStorage)。
    pub fn set_storage(&mut self, storage: Box<dyn Storage>) {
        self.storage = Some(storage);
    }

    // ---- 乱数 ----

    pub fn rnd_next(&mut self) -> u32 {
        let t = self.rnd_state[0] ^ (self.rnd_state[0].wrapping_shl(11));
        self.rnd_state[0] = self.rnd_state[1];
        self.rnd_state[1] = self.rnd_state[2];
        self.rnd_state[2] = self.rnd_state[3];
        let v = (self.rnd_state[3] ^ (self.rnd_state[3] >> 19)) ^ (t ^ (t >> 8));
        self.rnd_state[3] = v;
        v
    }

    pub fn random(&mut self, n: i16) -> i16 {
        let r = self.rnd_next();
        if n <= 0 {
            return 0;
        }
        ((r >> 1) % (n as u32)) as i16
    }

    pub fn random_seed(&mut self, n: i32) {
        self.rnd_state = [n as u32, 362436069, 521288629, 88675123];
    }

    // ---- 変数アクセス (VAR 領域の薄いラッパ) ----

    /// `i` は配列添字 0..102、102..128 が A..Z に対応。
    pub fn var_get(&self, i: usize) -> i16 {
        self.read_i16_le(OFFSET_RAM_VAR + i * 2)
    }

    pub fn var_set(&mut self, i: usize, v: i16) {
        self.write_i16_le(OFFSET_RAM_VAR + i * 2, v);
    }

    #[inline]
    fn read_i16_le(&self, off: usize) -> i16 {
        i16::from_le_bytes([self.ram[off], self.ram[off + 1]])
    }

    #[inline]
    fn write_i16_le(&mut self, off: usize, v: i16) {
        let b = v.to_le_bytes();
        self.ram[off] = b[0];
        self.ram[off + 1] = b[1];
    }

    pub fn clear_vars(&mut self) {
        self.ram[OFFSET_RAM_VAR..OFFSET_RAM_VAR + SIZE_RAM_VAR].fill(0);
    }

    // ---- LIST (プログラム領域) 操作 ----

    pub fn list_get_number(&self, index: u16) -> i16 {
        // POKE で LIST 終端 0 を消されると走査ループが LIST 領域外へ突き抜けるため、
        // 範囲外アクセスは終端 (行番号 0) として扱う。
        if (index as usize) + 2 > SIZE_RAM_LIST {
            return 0;
        }
        self.read_i16_le(OFFSET_RAM_LIST + index as usize)
    }

    pub fn list_set_number(&mut self, index: u16, num: i16) {
        self.write_i16_le(OFFSET_RAM_LIST + index as usize, num);
    }

    pub fn list_get_length(&self, index: u16) -> u8 {
        if (index as usize) + 3 > SIZE_RAM_LIST {
            return 0;
        }
        self.ram[OFFSET_RAM_LIST + index as usize + 2]
    }

    pub fn list_set_length(&mut self, index: u16, num: u8) {
        // num が奇数で 255 のとき num + (num & 1) が u8 をオーバーフローして
        // debug panic / release で誤値となるため saturating で防ぐ。
        let padded = num.saturating_add(num & 1);
        self.ram[OFFSET_RAM_LIST + index as usize + 2] = padded;
    }

    /// 行番号 number 以上の最初の行のインデックスを返す。
    pub fn list_find(&self, number: i16) -> u16 {
        let mut index: u16 = 0;
        loop {
            let n = self.list_get_number(index);
            if n == 0 || n >= number {
                return index;
            }
            index = index
                .wrapping_add(self.list_get_length(index) as u16)
                .wrapping_add(4);
        }
    }

    pub fn list_find_goto(&self, number: i16) -> i32 {
        let i = self.list_find(number);
        if self.list_get_number(i) != number {
            -1
        } else {
            i as i32
        }
    }

    pub fn list_set_pc(&mut self, n: u16) {
        self.pc = OFFSET_RAM_LIST + n as usize + 3;
    }

    /// 現在の PC が LIST 領域 (= プログラム本体) 内を指しているか。
    #[inline]
    pub fn pc_in_list(&self) -> bool {
        (OFFSET_RAM_LIST..OFFSET_RAM_LIST + SIZE_RAM_LIST).contains(&self.pc)
    }

    // ---- PEEK / POKE (仮想アドレス空間) ----

    pub fn peek(&self, ad: i32) -> u8 {
        if ad < 0 {
            return 0;
        }
        let uad = ad as usize;
        if uad < OFFSET_RAMROM {
            // 0x000-0x6FF: ROM フォント (0..0xE0 番文字 = 224 * 8 = 1792 byte)
            if uad < CHAR_PATTERN_JP.len() {
                CHAR_PATTERN_JP[uad]
            } else {
                0
            }
        } else if uad < OFFSET_RAMROM + SIZE_RAM {
            self.ram[uad - OFFSET_RAMROM]
        } else {
            0
        }
    }

    pub fn poke(&mut self, ad: i32, n: u8) {
        let ad = ad - OFFSET_RAMROM as i32;
        if ad >= 0 && (ad as usize) < SIZE_RAM {
            self.ram[ad as usize] = n;
        }
    }

    pub fn basic_init(&mut self) {
        self.clear_vars();
        self.ram[OFFSET_RAM_LIST..OFFSET_RAM_LIST + SIZE_RAM_LIST].fill(0);
        self.pc = PC_NULL;
        self.break_resume_pc = PC_NULL;
        self.list_size = 0;
        self.reset_pcg_to_font();
    }

    /// 実機の電源 ON/OFF による再起動と同一の効果を持つフル・リブート。
    /// `basic_init` (変数・プログラムのみ) と異なり、LED・画面・カナ入力・
    /// VIDEO 設定・音声・乱数シードなど電源断で失われるハードウェア状態も
    /// すべて起動直後の既定値へ戻す。`storage` (SAVE/LOAD スロット) はフラッシュ
    /// 相当で電源断でも消えないため、これだけ引き継ぐ。
    ///
    /// `BASIC` の `RESET` コマンド ([`crate::tokens::TOKEN_RESET`]) と、ホストが
    /// 提供する外部リセット API はどちらもこの一箇所に集約する。
    pub fn power_on_reset(&mut self) {
        let storage = self.storage.take();
        *self = Self::new();
        self.storage = storage;

        // `OK\n` は呼び出し元の文脈で出し方が異なる (BASIC の RESET 実行時は
        // 既存の "Execute → OK" 表示に任せ、ホスト起動時/外部リセット API では
        // 呼び出し側が明示的に出す) ため、ここではバナーのみ出力する。
        for c in BOOT_BANNER.bytes() {
            self.put_chr(c);
        }
    }

    // ---- エラー ----

    /// 直近の停止理由がエラー (または ESC ブレーク) だった場合の [`BasicError`]。
    /// `basic_start` 成功で `None` に戻る。ホストが構造化エラー通知に使う。
    pub fn last_error(&self) -> Option<BasicError> {
        self.last_error
    }

    /// 停止理由 `e` を画面に表示する。実行ループが `Err` を捕捉した時点で
    /// 1 度だけ呼ぶ。`is_quiet_mode` 中は何も表示しない。
    pub fn basic_print_error(&mut self, e: BasicError) {
        if self.is_quiet_mode {
            return;
        }
        if self.cursory == -1 {
            self.cursory = 0;
        }
        self.put_str(&e.to_string());

        if self.pc_in_list() {
            let mut index: u16 = 0;
            loop {
                let n = self.list_get_number(index);
                if n == 0 {
                    break;
                }
                let size = self.list_get_length(index) as usize;
                let line_end_in_ram = OFFSET_RAM_LIST + index as usize + size + 4;
                if self.pc <= line_end_in_ram {
                    let line_no = n;
                    let s = format!(" in {line_no}\n{line_no} ");
                    self.put_str(&s);
                    let mut p = OFFSET_RAM_LIST + index as usize + 3;
                    while p < self.ram.len() {
                        let c = self.ram[p];
                        if c == 0 {
                            break;
                        }
                        self.put_chr(c);
                        p += 1;
                    }
                    self.break_resume_pc = self.pc;
                    break;
                }
                index = index.wrapping_add(size as u16).wrapping_add(4);
            }
        }
        self.put_chr(b'\n');
    }

    // ---- 文字出力 ----

    pub fn put_chr(&mut self, c: u8) {
        self.screen_putc(c);
    }

    pub fn put_str(&mut self, s: &str) {
        for b in s.bytes() {
            self.put_chr(b);
        }
    }

    /// 数値を 10 進で表示して、表示桁数を返す
    pub fn put_num(&mut self, mut n: i32) -> u32 {
        let mut len = 0u32;
        if n < 0 {
            self.put_chr(b'-');
            len += 1;
            n = -n;
        }

        let mut v: u32 = 0;
        let mut d: u32 = 10000;
        while d > 0 {
            let c = (n as u32) / d;
            v |= c;
            if v != 0 || d == 1 {
                self.put_chr((c as u8) + b'0');
                len += 1;
            }
            n -= (c * d) as i32;
            d /= 10;
        }
        len
    }

    /// 10 進表示時の文字数 (符号 `-` を含む)。例: `decimal_width(-42)` → 3。
    /// PRINT DEC$ の桁数調整に使う。
    pub fn decimal_width(n: i32) -> u32 {
        let sign = u32::from(n < 0);
        let digits = n.unsigned_abs().checked_ilog10().unwrap_or(0) + 1;
        sign + digits
    }

    /// アドレス `n` (RAMROM 空間) から最大 `m` 文字を画面へ出力する。
    /// `"` か NUL に当たるか `m` 文字出したら止まる。
    pub fn put_str_from_mem(&mut self, n: i32, mut m: i16) {
        if n >= OFFSET_RAMROM as i32 {
            let mut p = (n - OFFSET_RAMROM as i32) as usize;
            while p < SIZE_RAM {
                let c = self.ram[p];
                if c == b'"' || c == 0 || m == 0 {
                    break;
                }
                self.put_chr(c);
                p += 1;
                m = m.saturating_sub(1);
            }
        }
    }

    /// ESC キーによる中断要求があるか。BASIC 実行ループの停止判定に使う。
    pub fn is_break_requested(&self) -> bool {
        self.is_esc_pressed
    }

    // ---- キーバッファ ----

    pub fn key_get_key(&mut self) -> i32 {
        match self.inkey_queue.pop_front() {
            Some(c) => c as i32,
            None => -1,
        }
    }

    pub fn key_clear_key(&mut self) {
        self.inkey_queue.clear();
        self.is_esc_pressed = false;
    }

    pub fn key_push(&mut self, c: u8) {
        if self.inkey_queue.len() < 126 {
            self.inkey_queue.push_back(c);
        }
    }

    /// 現在のキーボードレイアウト ID (0 = US, 1 = JA)。
    /// `KBD` コマンドで切替えられ、`VER(2)` の戻り値と一致する。
    pub fn keyboard_id(&self) -> u8 {
        self.keyboard_id
    }

    /// HID キーコード (USB usage ID 0..=0x67) と修飾キー状態から、現在の
    /// `keyboard_id` に応じた US/JA テーブルを引いて IchigoJam 内部コードを
    /// 返す。0 はそのキーに対する出力が無いことを表す (ホスト側で無視)。
    /// ホスト (eframe など物理キーが取れる UI) がキー入力を文字へ翻訳する
    /// 唯一の経路。
    pub fn keymap_lookup(&self, hid: u8, shift: bool, alt: bool) -> u8 {
        crate::keymap::lookup(self.keyboard_id, hid, shift, alt)
    }

    /// BTN() 用にキーの押下/解放状態を記録する。`code` はキーに対応する
    /// ASCII コード (矢印は 28-31、スペースは 32、英字は大文字コード等)。
    pub fn key_set_down(&mut self, code: u8, down: bool) {
        self.keys_down[code as usize] = down;
    }

    /// 全キーの押下状態をクリアする (ウィンドウのフォーカス喪失時など、
    /// 解放イベントを取りこぼしてキーが押しっぱなしになるのを防ぐ)。
    pub fn key_clear_down(&mut self) {
        self.keys_down = [false; 256];
    }

    /// BTN(n): キーボードを実機ボタンの代用にする。
    /// - `n == 0`: 実機の本体ボタン。デスクトップには無いので常に 0。
    /// - `n < 0`: 押下中キーのビットマスク
    ///   (bit0:← bit1:→ bit2:↑ bit3:↓ bit4:スペース bit5:X)。
    /// - `n > 0`: ASCII コード `n` のキーが押下中なら 1、そうでなければ 0。
    pub(crate) fn btn(&self, n: i16) -> i16 {
        use crate::keycodes as kc;
        if n == 0 {
            0
        } else if n < 0 {
            let bit = |code: u8, shift: u8| -> i16 { i16::from(self.is_key_down(code)) << shift };
            bit(kc::CURSOR_LEFT, 0)
                | bit(kc::CURSOR_RIGHT, 1)
                | bit(kc::CURSOR_UP, 2)
                | bit(kc::CURSOR_DOWN, 3)
                | bit(kc::SPACE, 4)
                | bit(kc::KEY_X, 5)
        } else {
            // n は ASCII コード。256 以上は対応キーが無いので 0。
            i16::from(u8::try_from(n).is_ok_and(|code| self.is_key_down(code)))
        }
    }

    fn is_key_down(&self, code: u8) -> bool {
        self.keys_down[code as usize]
    }

    // ---- ローマ字かな入力 ----

    /// カナモードを反転し、未確定バッファをクリアする。
    pub fn toggle_kana(&mut self) {
        self.is_kana_mode = !self.is_kana_mode;
        self.romaji_pending_0 = 0;
        self.romaji_pending_1 = 0;
    }

    /// 対話編集 (REPL) の各キー処理前に呼ぶ。挿入/上書きモードをユーザの
    /// トグル状態 `is_overwrite_toggle` に同期する。
    /// プログラム実行中の出力は basic_execute が上書きへ固定するので、
    /// ホストは実行中はこれを呼ばないこと。
    pub fn sync_insert_mode(&mut self) {
        self.is_overwrite_mode = self.is_overwrite_toggle;
    }

    /// カーソル描画幅。上書きモードは文字全体 (8px) を反転、挿入モードは
    /// 左半分 (4px) のみ反転する。true で全幅、false で左半分。
    pub fn cursor_full_width(&self) -> bool {
        self.is_overwrite_mode
    }

    /// プログラムを継続実行中か。対話編集 (入力・カーソル移動) を行わない
    /// 判定に使う。`pc` は STOP/ESC ブレーク後も CONT 用に保持されるため
    /// 判定には使えず、ホストが同期する [`Machine::is_program_running`] を見る。
    pub fn is_executing(&self) -> bool {
        self.is_program_running
    }

    /// `INPUT` 文がプロンプトを出して対話入力待ちに入っているか。
    /// ホストはこれが真の間、1 行入力を受け付け、確定時に
    /// [`Machine::input_complete`] を呼ぶ。
    pub fn is_awaiting_input(&self) -> bool {
        self.input_pending.is_some()
    }

    /// INPUT 入力待ちを代入せずに解除する (ESC 中断や NEW 等のリセット用)。
    pub fn cancel_input(&mut self) {
        self.input_pending = None;
    }

    /// 対話入力で得た 1 行 (`line`) を `INPUT` の代入先へ反映し、入力待ちを
    /// 解除する。`line` は IchigoJam 文字コードの生バイト列で、式として評価
    /// した結果を変数へ代入する。
    ///
    /// 評価には LINEBUF を一時利用するため、呼出前の LINEBUF 内容・トークナイザ
    /// 状態・`pc` を退避して評価後に復元する。これにより、即時モードで
    /// `INPUT` を実行して `pc` が LINEBUF 内にある場合でも再開位置が壊れない。
    /// パースに失敗した場合 (空入力など) は C の `errorignore` と同様に
    /// 代入をスキップし、変数は元の値のまま残す。
    pub fn input_complete(&mut self, line: &[u8]) {
        let Some(target) = self.input_pending.take() else {
            return;
        };

        let saved_pc = self.pc;
        let saved_token_start_pc = self.last_token_start_pc;
        let saved_token_end_pc = self.last_token_end_pc;
        let saved_bk = self.last_token;
        let saved_expr_mode = self.is_expr_mode;
        let saved_linebuf: Vec<u8> =
            self.ram[OFFSET_RAM_LINEBUF..OFFSET_RAM_LINEBUF + N_LINEBUF].to_vec();

        let max = N_LINEBUF.saturating_sub(1);
        let n = line.len().min(max);
        self.ram[OFFSET_RAM_LINEBUF..OFFSET_RAM_LINEBUF + n].copy_from_slice(&line[..n]);
        self.ram[OFFSET_RAM_LINEBUF + n] = 0;

        self.pc = OFFSET_RAM_LINEBUF;
        self.last_token_start_pc = 0;
        self.last_token_end_pc = 0;
        self.is_expr_mode = false;
        if let Ok(value) = self.eval_expression() {
            self.var_set(target, value);
        }

        self.ram[OFFSET_RAM_LINEBUF..OFFSET_RAM_LINEBUF + N_LINEBUF]
            .copy_from_slice(&saved_linebuf);
        self.pc = saved_pc;
        self.last_token_start_pc = saved_token_start_pc;
        self.last_token_end_pc = saved_token_end_pc;
        self.last_token = saved_bk;
        self.is_expr_mode = saved_expr_mode;

        self.put_chr(b'\n');
    }

    /// 対話編集用の制御コード入力 (矢印・BS・DEL・Home/End 等)。
    /// プログラム実行中はカーソル移動・画面編集を行わず無視する。
    /// 文字出力 (PRINT 等) は `put_chr`/`screen_putc` を直接使うため影響しない。
    pub fn input_control(&mut self, code: u8) {
        if self.is_executing() {
            return;
        }
        self.screen_putc(code);
    }

    /// テキスト入力 1 文字を画面へ反映する。
    /// カナモード ON の時はローマ字 → 半角カナへ変換し、BS による
    /// 直前文字の書き換えも含めて `screen_putc` へ流す。
    /// プログラム実行中は対話編集を行わないため無視する。
    pub fn input_putc(&mut self, c: u8) {
        if self.is_executing() {
            return;
        }
        if !self.is_kana_mode {
            self.screen_putc(c);
            return;
        }
        let mut buf0 = self.romaji_pending_0;
        let mut buf1 = self.romaji_pending_1;
        let out = crate::romajikana::romajikana_input(&mut buf0, &mut buf1, c);
        self.romaji_pending_0 = buf0;
        self.romaji_pending_1 = buf1;
        for &b in &out {
            self.screen_putc(b);
        }
    }
}

// ---- 共通ユーティリティ ----

#[inline]
pub fn basic_toupper(c: u8) -> u8 {
    c.to_ascii_uppercase()
}

/// C strlen8: NUL 終端文字列の長さ
pub fn strlen8(ram: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < ram.len() && ram[i] != 0 {
        i += 1;
    }
    i - start
}

/// IchigoJam の `calcDiv`: 切り捨て除算 (符号付き)。Rust 標準の `/` と同じ
/// 動作。呼出元がゼロ除算を防ぐ前提。
#[inline]
pub fn calc_div(n: i32, m: i32) -> i32 {
    n / m
}

#[inline]
pub fn calc_mod(n: i32, m: i32) -> i32 {
    n % m
}
