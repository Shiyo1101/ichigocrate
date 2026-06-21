//! IchigoJam 仮想マシンの中核状態。
//!
//! 元実装はグローバル変数の集合だが、本移植では `Machine` 構造体に集約し
//! `&mut self` 経由で操作する。

use std::collections::VecDeque;

use crate::errors::BasicError;
use crate::font::CHAR_PATTERN_JP;
use crate::ram::*;

pub const PC_NULL: usize = usize::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasicResult {
    StopOrErr,
    /// 正常終了 (呼び出し側は `OK` を表示する)
    Execute,
    /// 行番号付き入力により LIST が編集された (`OK` は表示しない)
    Edit,
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
    pub listsize: u16,

    // ===== スクリーン状態 (ホストが描画時に読む) =====
    pub cursorx: i32,
    pub cursory: i32,
    pub cursorflg: bool,
    pub screen_invert: bool,
    /// 拡大表示の段階 (VIDEO 3/4 で 1 以上)。表示倍率は `1 << screen_big`。
    /// 0 = 等倍, 1 = 2 倍, 2 = 4 倍, 3 = 8 倍 (最大 3 でクリップ)。
    pub screen_big: u8,
    /// 映像出力の有効/無効 (VIDEO 0 でオフ)。ホストはオフ時に黒画面を描画する。
    pub video_enabled: bool,

    // ===== キーボード関連 (ホストが書く) =====
    pub key_kana: bool,
    pub key_flg_esc: i8,

    // ===== タイマ (ホストが 60Hz で更新) =====
    pub frames: u16,

    // ===== I/O 状態 (ホストが LED 枠線色決定で読む) =====
    pub led: bool,

    // ===== サウンド出力 (UI/Audio スレッド連携) =====
    /// 現在の周波数 (Hz)。0 なら無音。
    pub current_tone_hz: f32,

    // ===== WAIT 用フレームカウンタ (協調的待機) =====
    /// 残り待機フレーム数。0 でなければ basic_step は即 return する。
    pub wait_frames: u32,

    // ---- 以下はクレート内専用 (`pub(crate)`) ----

    /// `token_back` 用に直前のトークン取得開始位置を覚える
    pub(crate) lasttoken: usize,
    pub(crate) lasttokenpc: usize,
    pub(crate) bklasttoken: Token,
    pub(crate) pcbreak: usize,
    /// 直近の停止理由。実行ループ ([`Machine::basic_step`]) が `Err` を捕捉して
    /// 格納し、境界 (`exec_line`) が読み取る。
    pub(crate) last_error: Option<BasicError>,
    pub(crate) ngosubstack: u8,
    pub(crate) nforstack: u8,
    pub(crate) gosubstack: [usize; IJB_SIZEOF_GOSUB_STACK],
    pub(crate) forstack: [usize; IJB_SIZEOF_FOR_STACK],
    /// 0:コマンド 1:式
    pub(crate) tokenmode: u8,

    pub(crate) screenw: usize,
    pub(crate) screenh: usize,
    pub(crate) screen_insertmode: bool,
    pub(crate) screen_locatemode: u8,

    pub(crate) key_insert: bool,
    /// ローマ字かな変換の未確定バッファ (子音 1〜2 文字目)
    pub(crate) key_kana_buf_0: u8,
    pub(crate) key_kana_buf_1: u8,
    /// INKEY() 用のキューイング入力バッファ
    pub(crate) keybuf: VecDeque<u8>,
    /// キーボードレイアウト ID (`KBD` コマンド / `VER(2)` 用)。
    /// 0 = US, 1 = JA。`KBD n` は `!!n` で正規化される。実機はフラッシュへ
    /// 永続化するが本移植はメモリ内のみ。
    pub(crate) keyboard_id: u8,
    /// 現在押下中のキー (BTN() 用)。ASCII コードで索引する押下フラグ。
    /// ホストがキー押下/解放ごとに [`Machine::key_set_down`] で更新する。
    pub(crate) keys_down: [bool; 256],
    /// プログラムを継続実行中か。ホスト (アプリ) が毎フレーム自身の実行
    /// ループ状態と同期させる。RUN 中は true、END/STOP/ESC ブレーク/完了で
    /// false。`pc` は STOP/ブレーク後も CONT 用に保持されるため実行中判定には
    /// 使えない。対話編集の入力可否はこのフラグで判断する。
    pub program_running: bool,
    pub(crate) noresmode: bool,

    pub(crate) psgoct: u8,
    pub(crate) psgdeflen: u8,
    pub(crate) psgratio: u8,
    pub(crate) psgwaitcnt: u16,
    pub(crate) psgtone: u16,
    pub(crate) psgtempo: u16,
    pub(crate) psglen: u32,
    /// MML 文字列の RAM インデックス (None = 演奏終了)
    pub(crate) psgmml: Option<usize>,
    pub(crate) psgrep: Option<usize>,

    pub(crate) linecnt: u16,
    pub(crate) rndn: [u32; 4],

    /// 最後に SAVE/LOAD した slot 番号 (FILE() で参照)
    pub(crate) lastfile: u8,

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
            lasttoken: 0,
            lasttokenpc: 0,
            bklasttoken: Token::default(),
            pcbreak: PC_NULL,
            last_error: None,
            ngosubstack: 0,
            nforstack: 0,
            gosubstack: [0; IJB_SIZEOF_GOSUB_STACK],
            forstack: [0; IJB_SIZEOF_FOR_STACK],
            tokenmode: 0,
            listsize: 0,

            cursorx: 0,
            cursory: 0,
            screenw: SCREEN_W,
            screenh: SCREEN_H,
            cursorflg: true,
            screen_insertmode: true,
            screen_locatemode: 0,
            screen_invert: false,
            screen_big: 0,
            video_enabled: true,

            // key_insert は 0=挿入 / 1=上書き (移植元の流儀をそのまま使う)
            key_insert: false,
            key_kana: false,
            key_kana_buf_0: 0,
            key_kana_buf_1: 0,
            key_flg_esc: 0,
            keybuf: VecDeque::with_capacity(128),
            keyboard_id: 0,
            keys_down: [false; 256],
            program_running: false,
            noresmode: false,

            psgoct: 3,
            psgdeflen: 8,
            psgratio: 1,
            psgwaitcnt: 0,
            psgtone: 0,
            psgtempo: 120,
            psglen: 0,
            psgmml: None,
            psgrep: None,

            frames: 0,
            linecnt: 0,
            rndn: [123456789, 362436069, 521288629, 88675123],

            led: false,
            lastfile: 0,

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

    // ---- 乱数 (random.h より) ----

    pub fn rnd_next(&mut self) -> u32 {
        let t = self.rndn[0] ^ (self.rndn[0].wrapping_shl(11));
        self.rndn[0] = self.rndn[1];
        self.rndn[1] = self.rndn[2];
        self.rndn[2] = self.rndn[3];
        let v = (self.rndn[3] ^ (self.rndn[3] >> 19)) ^ (t ^ (t >> 8));
        self.rndn[3] = v;
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
        self.rndn = [n as u32, 362436069, 521288629, 88675123];
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
        self.read_i16_le(OFFSET_RAM_LIST + index as usize)
    }

    pub fn list_set_number(&mut self, index: u16, num: i16) {
        self.write_i16_le(OFFSET_RAM_LIST + index as usize, num);
    }

    pub fn list_get_length(&self, index: u16) -> u8 {
        self.ram[OFFSET_RAM_LIST + index as usize + 2]
    }

    pub fn list_set_length(&mut self, index: u16, num: u8) {
        self.ram[OFFSET_RAM_LIST + index as usize + 2] = num + (num & 1);
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
        self.pcbreak = PC_NULL;
        self.listsize = 0;
        self.screen_clp();
    }

    // ---- エラー ----

    /// 停止理由 `e` を画面に表示する。実行ループが `Err` を捕捉した時点で
    /// 1 度だけ呼ぶ。`noresmode` 中は何も表示しない。
    pub fn basic_print_error(&mut self, e: BasicError) {
        if self.noresmode {
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
                    let s = format!(" in {}\n{} ", line_no, line_no);
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
                    self.pcbreak = self.pc;
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

    /// 10 進表示時の文字数 (符号 `-` を含む)。例: `beam(-42)` → 3。
    /// PRINT DEC$ の桁数調整に使う。
    pub fn beam(n: i32) -> u32 {
        let sign = u32::from(n < 0);
        let digits = n.unsigned_abs().checked_ilog10().unwrap_or(0) + 1;
        sign + digits
    }

    pub fn put_strmem(&mut self, n: i32, mut m: i16) {
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
    pub fn stop_execute(&self) -> bool {
        self.key_flg_esc != 0
    }

    // ---- キーバッファ ----

    pub fn key_get_key(&mut self) -> i32 {
        match self.keybuf.pop_front() {
            Some(c) => c as i32,
            None => -1,
        }
    }

    pub fn key_clear_key(&mut self) {
        self.keybuf.clear();
        self.key_flg_esc = 0;
    }

    pub fn key_push(&mut self, c: u8) {
        if self.keybuf.len() < 126 {
            self.keybuf.push_back(c);
        }
    }

    /// 現在のキーボードレイアウト ID (0 = US, 1 = JA)。
    /// `KBD` コマンドで切替えられ、`VER(2)` の戻り値と一致する。
    pub fn keyboard_id(&self) -> u8 {
        self.keyboard_id
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
        self.key_kana = !self.key_kana;
        self.key_kana_buf_0 = 0;
        self.key_kana_buf_1 = 0;
    }

    /// 対話編集 (REPL) の各キー処理前に呼ぶ。挿入/上書きモードをユーザの
    /// トグル状態 `key_insert` (false=挿入, true=上書き) に同期する。
    /// プログラム実行中の出力は basic_execute が上書きへ固定するので、
    /// ホストは実行中はこれを呼ばないこと。
    pub fn sync_insert_mode(&mut self) {
        self.screen_insertmode = self.key_insert;
    }

    /// カーソル描画幅。上書きモードは文字全体 (8px) を反転、挿入モードは
    /// 左半分 (4px) のみ反転する (実機準拠)。true で全幅、false で左半分。
    pub fn cursor_full_width(&self) -> bool {
        self.screen_insertmode
    }

    /// プログラムを継続実行中か。対話編集 (入力・カーソル移動) を行わない
    /// 判定に使う。`pc` は STOP/ESC ブレーク後も CONT 用に保持されるため
    /// 判定には使えず、ホストが同期する [`Machine::program_running`] を見る。
    pub fn is_executing(&self) -> bool {
        self.program_running
    }

    /// 対話編集用の制御コード入力 (矢印・BS・DEL・Home/End 等)。
    /// プログラム実行中はカーソル移動・画面編集を行わず無視する (実機準拠)。
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
        if !self.key_kana {
            self.screen_putc(c);
            return;
        }
        let mut buf0 = self.key_kana_buf_0;
        let mut buf1 = self.key_kana_buf_1;
        let out = crate::romajikana::romajikana_input(&mut buf0, &mut buf1, c);
        self.key_kana_buf_0 = buf0;
        self.key_kana_buf_1 = buf1;
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
