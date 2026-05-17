//! IchigoJam 仮想マシンの中核状態。
//!
//! IchigoJam の元 C 実装はグローバル変数の集合だが、本移植では `Machine`
//! 構造体に集約し、`&mut self` 経由で操作する。

use std::collections::VecDeque;

use crate::errors::*;
use crate::font::CHAR_PATTERN_JP;
use crate::ram::*;

pub const PC_NULL: usize = usize::MAX;

/// BASIC 実行結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasicResult {
    /// 停止 (エラーまたは Break)
    StopOrErr,
    /// 正常終了 (`OK` 表示すべき場合)
    Execute,
    /// 行編集 (行番号付き入力が行われた)
    Edit,
}

/// 1 トークン
#[derive(Debug, Clone, Copy, Default)]
pub struct Token {
    pub code: u16,
    pub value: i16,
}

/// IchigoJam BASIC 仮想マシン
pub struct Machine {
    /// 統合 RAM (PCG/VAR/VRAM/LIST/KEYBUF/LINEBUF/I2CBUF)
    pub ram: Vec<u8>,
    /// プログラムカウンタ (ram のインデックス)。PC_NULL なら未走行。
    pub pc: usize,
    /// 直前のトークン取得開始位置 (token_back 用)
    pub lasttoken: usize,
    /// 直前のトークン取得後の位置
    pub lasttokenpc: usize,
    /// 直前のトークン
    pub bklasttoken: Token,
    /// ブレーク時に保持される pc
    pub pcbreak: usize,
    /// エラー番号 (0 なら無エラー)
    pub err: u8,
    /// GOSUB スタックの段数
    pub ngosubstack: u8,
    /// FOR スタックの段数
    pub nforstack: u8,
    pub gosubstack: [usize; IJB_SIZEOF_GOSUB_STACK],
    pub forstack: [usize; IJB_SIZEOF_FOR_STACK],
    /// トークンモード (0:コマンド 1:式)
    pub tokenmode: u8,
    /// LIST の使用バイト数 (末尾 0x00 0x00 を除く)
    pub listsize: u16,

    // ===== スクリーン状態 =====
    pub cursorx: i32,
    pub cursory: i32,
    pub screenw: usize,
    pub screenh: usize,
    pub cursorflg: bool,
    pub screen_insertmode: bool,
    pub screen_locatemode: u8,
    pub screen_invert: bool,
    pub screen_big: u8,

    // ===== キーボード関連 =====
    pub key_insert: bool,
    pub key_bkinsert: bool,
    pub key_kana: bool,
    pub key_flg_esc: i8,
    /// 押下キーバッファ (REPL/INPUT/INKEY 用)
    pub keybuf: VecDeque<u8>,
    pub errorignore: bool,
    pub noresmode: bool,

    // ===== PSG (音源) =====
    pub psgoct: u8,
    pub psgdeflen: u8,
    pub psg_sounder: u8,
    pub psgratio: u8,
    pub psgwaitcnt: u16,
    pub psgtone: u16,
    pub psgtempo: u16,
    pub psglen: u32,
    /// MML 文字列の RAM インデックス (None = 演奏終了)
    pub psgmml: Option<usize>,
    pub psgrep: Option<usize>,

    // ===== タイマ / 乱数 =====
    pub frames: u16,
    pub linecnt: u16,
    pub rndn: [u32; 4],

    // ===== I/O 状態 =====
    pub led: bool,
    pub sleepflg: u8,
    pub fileslot: u8,
    pub lastfile: u8,
    pub uartmode_txd: u8,
    pub uartmode_rxd: u8,

    // ===== サウンド出力 (UI/Audio スレッド連携) =====
    /// 現在の周波数 (Hz)。0 なら無音。
    pub current_tone_hz: f32,

    // ===== WAIT 用フレームカウンタ (協調的待機) =====
    /// 残り待機フレーム数。0 でなければ basic_step は即 return する。
    pub wait_frames: u32,

    // ===== ストレージ (SAVE/LOAD/FILES) =====
    /// ホスト側ストレージ (デスクトップではディスク)。None なら File error。
    pub storage: Option<Box<dyn Storage>>,
}

/// SAVE/LOAD/FILES のホスト側実装。デスクトップアプリは実ファイル、
/// テスト/組込はメモリ実装を差し込む。
pub trait Storage {
    /// 指定スロットへ data 全体を保存。成功なら true。
    fn save(&mut self, slot: u8, data: &[u8]) -> bool;
    /// 指定スロットから最大 buf.len() バイトを読み出し、読込バイト数を返す。
    /// スロットが空・読込エラーなら -1。
    fn load(&mut self, slot: u8, buf: &mut [u8]) -> i32;
    /// FILES 用: スロットの先頭バイト (LIST 形式) を覗き見る。
    /// 戻り値は読込バイト数 (`load` と同じ意味)。
    fn peek(&mut self, slot: u8, buf: &mut [u8]) -> i32;
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
            err: 0,
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

            key_insert: true,
            key_bkinsert: true,
            key_kana: false,
            key_flg_esc: 0,
            keybuf: VecDeque::with_capacity(128),
            errorignore: false,
            noresmode: false,

            psgoct: 3,
            psgdeflen: 8,
            psg_sounder: 0,
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
            sleepflg: 0,
            fileslot: 0,
            lastfile: 0,
            uartmode_txd: 0,
            uartmode_rxd: 0,

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

    // ============================================================
    // 乱数 (random.h より)
    // ============================================================

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

    // ============================================================
    // 変数アクセス (VAR 領域への薄いラッパ)
    // ============================================================

    /// 変数 var\[i\] を取得 (i は配列添字 0..102, 102..128 が A..Z)
    pub fn var_get(&self, i: usize) -> i16 {
        let off = OFFSET_RAM_VAR + i * 2;
        i16::from_le_bytes([self.ram[off], self.ram[off + 1]])
    }

    pub fn var_set(&mut self, i: usize, v: i16) {
        let off = OFFSET_RAM_VAR + i * 2;
        let b = v.to_le_bytes();
        self.ram[off] = b[0];
        self.ram[off + 1] = b[1];
    }

    pub fn clear_vars(&mut self) {
        for b in &mut self.ram[OFFSET_RAM_VAR..OFFSET_RAM_VAR + SIZE_RAM_VAR] {
            *b = 0;
        }
    }

    // ============================================================
    // LIST (プログラム領域) 操作
    // ============================================================

    pub fn list_get_number(&self, index: u16) -> i16 {
        let off = OFFSET_RAM_LIST + index as usize;
        i16::from_le_bytes([self.ram[off], self.ram[off + 1]])
    }

    pub fn list_set_number(&mut self, index: u16, num: i16) {
        let off = OFFSET_RAM_LIST + index as usize;
        let b = num.to_le_bytes();
        self.ram[off] = b[0];
        self.ram[off + 1] = b[1];
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

    // ============================================================
    // PEEK / POKE (仮想アドレス空間)
    // ============================================================

    pub fn peek(&self, ad: i32) -> u8 {
        let ad = ad;
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

    // ============================================================
    // 基本初期化
    // ============================================================

    pub fn basic_init(&mut self) {
        self.clear_vars();
        for b in &mut self.ram[OFFSET_RAM_LIST..OFFSET_RAM_LIST + SIZE_RAM_LIST] {
            *b = 0;
        }
        self.pc = PC_NULL;
        self.pcbreak = PC_NULL;
        self.listsize = 0;
        // PCG ロード (CHAR_PATTERN の末尾 32 文字をコピー)
        self.screen_clp();
    }

    // ============================================================
    // エラー
    // ============================================================

    pub fn command_error(&mut self, err: u8) {
        if self.errorignore {
            return;
        }
        self.err = err;
        self.basic_print_error();
    }

    pub fn basic_print_error(&mut self) {
        if self.noresmode {
            return;
        }
        if self.cursory == -1 {
            self.cursory = 0;
        }
        if (self.err as usize) < ERR_MESSAGES.len() {
            let msg = ERR_MESSAGES[self.err as usize];
            // メッセージは別途確保 (借用衝突回避)
            let s = msg.to_string();
            self.put_str(&s);
        }

        // 実行中の行番号を表示
        if self.pc >= OFFSET_RAM_LIST && self.pc < OFFSET_RAM_LIST + SIZE_RAM_LIST {
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
                    // 行文字列
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
        // psg_beep(10, 3) は省略 (エラー音はオプション)
    }

    // ============================================================
    // 文字出力 (put_chr / put_str / put_num)
    // ============================================================

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

    pub fn beam(n: i32) -> u32 {
        let mut res: u32 = 1;
        let mut n = n;
        if n < 0 {
            res += 1;
            n = -n;
        }
        let mut chk: i32 = 10;
        while n >= chk {
            res += 1;
            chk *= 10;
        }
        res
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

    // ============================================================
    // ESC 確認 (Break)
    // ============================================================

    /// ESC キーが押されているかを確認 (BASIC 実行のループ判定用)
    pub fn stop_execute(&self) -> bool {
        self.key_flg_esc != 0
    }

    // ============================================================
    // キーバッファ
    // ============================================================

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
}

// ============================================================
// ユーティリティ
// ============================================================

#[inline]
pub fn basic_toupper(c: u8) -> u8 {
    if (b'a'..=b'z').contains(&c) {
        c & 0b1011111
    } else {
        c
    }
}

/// C strlen8: NUL 終端文字列の長さ
pub fn strlen8(ram: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < ram.len() && ram[i] != 0 {
        i += 1;
    }
    i - start
}

/// IchigoJam の calcDiv: 切り捨て除算 (符号付き、ゼロ除算は呼ばない前提)
#[inline]
pub fn calc_div(n: i32, m: i32) -> i32 {
    let mut flg = 1i32;
    let mut n = n;
    let mut m = m;
    if n < 0 {
        n = -n;
        flg = -flg;
    }
    if m < 0 {
        m = -m;
        flg = -flg;
    }
    ((n as u32) / (m as u32)) as i32 * flg
}

#[inline]
pub fn calc_mod(n: i32, m: i32) -> i32 {
    let d = calc_div(n, m);
    n - d * m
}
