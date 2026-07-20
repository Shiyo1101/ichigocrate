// Copyright 2014-2024 the IchigoJam authors. All rights reserved. MIT license.
// https://github.com/IchigoJam/ichigojam-firm/blob/main/IchigoJam_BASIC/screen.h

//! VRAM 文字操作およびピクセル描画。

use crate::font::CHAR_PATTERN_JP;
use crate::keycodes::{
    BACKSPACE, CURSOR_DOWN, CURSOR_LEFT, CURSOR_RIGHT, CURSOR_UP, DELETE, FORM_FEED,
    INSERT_TOGGLE, KANA_TOGGLE, LINE_SPLIT, LOCATE_PREFIX,
};
use crate::machine::{calc_div, Machine};
use crate::ram::*;

// screen_scroll の方向 (LOCATE/SCROLL 経由で渡される数値)。値はカーソル
// コードと一致するが、意味が異なる別ドメインなので別途定義する。
const SCROLL_LEFT: i32 = 28;
const SCROLL_RIGHT: i32 = 29;
const SCROLL_UP: i32 = 30;
const SCROLL_DOWN: i32 = 31;

/// PCG 領域に格納される 0x80..0x90 の特殊文字範囲 (ピクセル描画用)。
const PCG_PIXEL_BASE: u8 = 128;
const PCG_PIXEL_RANGE: std::ops::Range<u8> = PCG_PIXEL_BASE..(PCG_PIXEL_BASE + 16);

impl Machine {
    pub fn vram(&self) -> &[u8] {
        &self.ram[OFFSET_RAM_VRAM..OFFSET_RAM_VRAM + SIZE_RAM_VRAM]
    }

    pub fn vram_mut(&mut self) -> &mut [u8] {
        &mut self.ram[OFFSET_RAM_VRAM..OFFSET_RAM_VRAM + SIZE_RAM_VRAM]
    }

    pub fn pcg(&self) -> &[u8] {
        &self.ram[OFFSET_RAM_PCG..OFFSET_RAM_PCG + SIZE_RAM_PCG]
    }

    /// CLP: PCG をフォントの末尾 32 文字で初期化する。
    pub fn reset_pcg_to_font(&mut self) {
        let src = &CHAR_PATTERN_JP[(0x100 - SIZE_PCG) * 8..(0x100 * 8)];
        self.ram[OFFSET_RAM_PCG..OFFSET_RAM_PCG + SIZE_RAM_PCG].copy_from_slice(src);
    }

    pub fn screen_clear(&mut self) {
        self.cursorx = 0;
        self.cursory = 0;
        self.vram_mut().fill(0);
    }

    /// 表示中の論理画面の桁数 (拡大時は縮む)。ホストの描画ループが参照する。
    pub fn screen_cols(&self) -> usize {
        self.text_cols
    }

    /// 表示中の論理画面の行数 (拡大時は縮む)。
    pub fn screen_rows(&self) -> usize {
        self.text_rows
    }

    /// VIDEO オン処理。拡大段階に合わせて論理画面サイズを
    /// `SCREEN_W/H >> screen_big` に再設定する。これにより折り返し位置・
    /// カーソル可動範囲が拡大倍率へ追従する。
    pub fn video_on(&mut self) {
        self.is_video_enabled = true;
        self.text_cols = SCREEN_W >> self.screen_big as u32;
        self.text_rows = SCREEN_H >> self.screen_big as u32;
    }

    /// CLT: TICK() が返すフレームカウンタ (`frames`) と行カウンタ (`video_line_count`) を 0 に戻す。
    pub fn reset_tick_counters(&mut self) {
        self.frames = 0;
        self.video_line_count = 0;
    }

    /// TICK(n) の現在値を返す (n=0: フレームカウンタ, n≠0: 行カウンタ)。
    pub fn tick_count(&self, n: i16) -> i16 {
        let v = if n != 0 { self.video_line_count } else { self.frames };
        (v & 0x7fff) as i16
    }

    pub fn screen_get(&self, x: i32, y: i32) -> u8 {
        if x < 0 || x >= self.text_cols as i32 || y < 0 || y >= self.text_rows as i32 {
            return 0;
        }
        self.vram()[(y as usize) * self.text_cols + x as usize]
    }

    pub fn screen_get_current(&self) -> u8 {
        self.screen_get(self.cursorx, self.cursory)
    }

    pub fn screen_locate(&mut self, mut x: i32, mut y: i32) {
        if x < 0 {
            x = 0;
        } else if x >= self.text_cols as i32 {
            x = self.text_cols as i32 - 1;
        }
        if y < 0 {
            y = -1;
        } else if y >= self.text_rows as i32 {
            y = self.text_rows as i32 - 1;
        }
        self.cursorx = x;
        self.cursory = y;
    }

    pub fn screen_scroll(&mut self, n: i32) {
        let w = self.text_cols;
        let h = self.text_rows;
        // SCROLL コマンドは 0..=3 と 28..=31 のどちらの形式でも受ける。
        let dir = match n {
            0 | SCROLL_UP => SCROLL_UP,
            1 | SCROLL_RIGHT => SCROLL_RIGHT,
            2 | SCROLL_DOWN => SCROLL_DOWN,
            3 | SCROLL_LEFT => SCROLL_LEFT,
            _ => return,
        };
        let v = self.vram_mut();
        match dir {
            SCROLL_UP => {
                v.copy_within(w..w * h, 0);
                v[(h - 1) * w..h * w].fill(0);
            }
            SCROLL_RIGHT => {
                for i in (0..w - 1).rev() {
                    for j in 0..h {
                        v[j * w + (i + 1)] = v[j * w + i];
                    }
                }
                for j in 0..h {
                    v[j * w] = 0;
                }
            }
            SCROLL_DOWN => {
                for i in (0..h - 1).rev() {
                    v.copy_within(i * w..(i + 1) * w, (i + 1) * w);
                }
                v[0..w].fill(0);
            }
            SCROLL_LEFT => {
                for i in 1..w {
                    for j in 0..h {
                        v[j * w + (i - 1)] = v[j * w + i];
                    }
                }
                for j in 0..h {
                    v[j * w + (w - 1)] = 0;
                }
            }
            _ => {}
        }
    }

    fn screen_enter(&mut self) {
        if self.cursory == self.text_rows as i32 - 1 {
            self.screen_scroll(0);
        } else {
            self.cursory += 1;
        }
        self.cursorx = 0;
    }

    /// UP/DOWN でカーソルを縦移動した後、テキストエディタのように入力済み
    /// 領域の末尾へカーソルを引き戻す。挿入モードでカーソル先が空白セルの
    /// とき、左隣が空でなくなる位置 (= テキスト末尾) まで、なければ 0 列まで
    /// 戻す。上書きモードは実機同様に自由移動とし、何もしない。
    fn cursor_snap_to_text(&mut self) {
        if self.is_overwrite_mode {
            return; // 上書きモードは自由移動
        }
        let w = self.text_cols;
        let row = self.cursory as usize * w;
        // カーソル先に文字があるならスナップ不要 (テキスト上)
        if self.vram()[row + self.cursorx as usize] != 0 {
            return;
        }
        // 左隣が空白の間だけ戻る。0 列へはいくらでも到達できる。
        while self.cursorx > 0 && self.vram()[row + self.cursorx as usize - 1] == 0 {
            self.cursorx -= 1;
        }
    }

    /// 通常の文字描画に加え、改行・カーソル移動・編集系の制御コード
    /// ([`crate::keycodes`]) と LOCATE 連動シーケンスをすべてここで処理する。
    pub fn screen_putc(&mut self, c: u8) {
        if self.locate_pending_bytes != 0 {
            if self.locate_pending_bytes == 2 {
                if c < 32 {
                    self.screen_scroll(c as i32);
                    self.locate_pending_bytes -= 1;
                } else {
                    self.screen_locate(c as i32 - 32, self.cursory);
                }
            } else {
                self.screen_locate(self.cursorx, c as i32 - 32);
            }
            self.locate_pending_bytes = self.locate_pending_bytes.saturating_sub(1);
            return;
        }
        if self.cursory == -1 {
            return;
        }
        let w = self.text_cols;
        let h = self.text_rows;

        match c {
            b'\r' => {}
            b'\n' => {
                loop {
                    let cy = self.cursory;
                    let line_end = (cy as usize + 1) * w - 1;
                    if self.vram()[line_end] == 0 {
                        break;
                    } else if cy == h as i32 - 1 {
                        self.screen_enter();
                        break;
                    }
                    self.cursory += 1;
                }
                self.screen_enter();
            }
            b'\t' => {
                self.screen_putc(b' ');
                self.screen_putc(b' ');
            }
            BACKSPACE | DELETE => {
                // DEL: その場の文字を詰めるだけでカーソルは動かさない。
                // BS : 1 文字前へ戻った上で詰める。
                if c == BACKSPACE {
                    if self.cursorx > 0 {
                        self.cursorx -= 1;
                    } else if self.cursory > 0
                        && self.vram()[self.cursory as usize * w - 1] != 0
                    {
                        self.cursory -= 1;
                        self.cursorx = w as i32 - 1;
                    } else {
                        return;
                    }
                }
                let mut i = self.cursory as usize * w + self.cursorx as usize;
                let last = w * h - 1;
                while i < last {
                    let v = self.vram_mut();
                    if v[i] == 0 {
                        break;
                    }
                    v[i] = v[i + 1];
                    i += 1;
                }
                if i == last {
                    self.vram_mut()[i] = 0;
                }
            }
            CURSOR_LEFT => {
                if self.cursorx > 0 {
                    self.cursorx -= 1;
                } else if self.cursory > 0
                    && (self.vram()[self.cursory as usize * w - 1] != 0
                        || self.is_overwrite_mode)
                {
                    self.cursory -= 1;
                    self.cursorx = w as i32 - 1;
                }
            }
            CURSOR_RIGHT => {
                let at_last_cell =
                    self.cursorx == w as i32 - 1 && self.cursory == h as i32 - 1;
                let current_idx = self.cursory as usize * w + self.cursorx as usize;
                let movable = !at_last_cell
                    && (self.vram()[current_idx] != 0 || self.is_overwrite_mode);
                if movable {
                    self.cursorx += 1;
                    if self.cursorx == w as i32 {
                        self.cursorx = 0;
                        self.cursory += 1;
                    }
                }
            }
            CURSOR_UP => {
                if self.cursory > 0 {
                    self.cursory -= 1;
                    self.cursor_snap_to_text();
                }
            }
            CURSOR_DOWN => {
                if self.cursory < h as i32 - 1 {
                    self.cursory += 1;
                    self.cursor_snap_to_text();
                }
            }
            LINE_SPLIT => {
                self.screen_enter();
            }
            FORM_FEED => {
                let now = self.cursory as usize * w + self.cursorx as usize;
                self.vram_mut()[now..w * h].fill(0);
            }
            LOCATE_PREFIX => {
                self.locate_pending_bytes = 2;
            }
            KANA_TOGGLE => {
                self.is_kana_mode = !self.is_kana_mode;
            }
            INSERT_TOGGLE => {
                self.is_overwrite_toggle = !self.is_overwrite_toggle;
            }
            _ => {
                if c < 32 && c != 0 {
                    return;
                }
                if !self.is_overwrite_mode {
                    // 挿入モード: 後続の文字を 1 つずつ右へずらして空きを作る
                    let mut now = self.cursory as usize * w + self.cursorx as usize;
                    let mut cxlast = now;
                    while cxlast < w * h && self.vram()[cxlast] != 0 {
                        cxlast += 1;
                    }
                    if cxlast == w * h {
                        if self.cursory > 0 {
                            self.screen_scroll(0);
                            self.cursory -= 1;
                            now -= w;
                            cxlast -= w;
                        } else {
                            cxlast -= 1;
                        }
                    }
                    let v = self.vram_mut();
                    for i in (now..cxlast).rev() {
                        v[i + 1] = v[i];
                    }
                    v[now] = c;
                    self.cursorx += 1;
                    if self.cursorx == w as i32 {
                        self.cursorx = 0;
                        if self.cursory < h as i32 - 1 {
                            self.cursory += 1;
                        } else {
                            self.screen_enter();
                        }
                    }
                } else {
                    // 上書きモード
                    let idx = self.cursory as usize * w + self.cursorx as usize;
                    self.vram_mut()[idx] = c;
                    self.cursorx += 1;
                    if self.cursorx == w as i32 {
                        self.screen_enter();
                    }
                }
            }
        }
    }

    /// 現在カーソルがある論理行 (折り返しで上へ続く行も遡る) の先頭の
    /// RAM インデックスを返す (Enter 確定時の行読み取り用)。
    pub fn screen_line_start(&mut self) -> usize {
        if self.cursory == -1 {
            self.cursory = 0;
            self.cursorx = 0;
        }
        let w = self.text_cols;
        let p = (self.cursory as i64 - 1) * w as i64;
        if p < 0 {
            return OFFSET_RAM_VRAM;
        }
        let mut p = p as usize;
        // 上行末尾が直前行のテキスト末尾になるよう 1 行戻す
        // (空白セルの直前に文字が来ている場合のみ)
        let v = self.vram();
        if v[p] == 0 && p > 0 && v[p - 1] != 0 {
            p = p.saturating_sub(w);
        }
        while v[p] != 0 {
            if p == 0 {
                break;
            }
            p -= 1;
        }
        if v[p] != 0 {
            OFFSET_RAM_VRAM + p
        } else {
            OFFSET_RAM_VRAM + p + 1
        }
    }

    /// 現在カーソルがある論理行を消去し、カーソルを行頭へ移動する。
    /// F キーのコマンド (LIST/RUN など) は、行に何が書かれていても上書き
    /// 表示・実行できる必要があるため、書き込み前にこれで行を空にする。
    /// 折り返しで複数行に跨る論理行は、行末セルが埋まって次行へ continue
    /// している連続行をまとめて消す。
    pub fn screen_clear_line(&mut self) {
        if self.cursory < 0 {
            return;
        }
        let w = self.text_cols;
        let h = self.text_rows;
        // 直前行の末尾セルが埋まっている = 折り返し継続なので行頭まで遡る。
        let mut top = self.cursory as usize;
        while top > 0 && self.vram()[top * w - 1] != 0 {
            top -= 1;
        }
        // 自行の末尾セルが埋まっている間は折り返しが続くので下へ伸ばす。
        let mut bottom = top;
        while bottom + 1 < h && self.vram()[(bottom + 1) * w - 1] != 0 {
            bottom += 1;
        }
        self.vram_mut()[top * w..(bottom + 1) * w].fill(0);
        self.cursorx = 0;
        self.cursory = top as i32;
    }

    // ---- ピクセル描画 (DRAW / POINT) ----

    pub fn screen_pset(&mut self, x: i32, y: i32, cmd: i32) -> u32 {
        let w = self.text_cols as i32;
        let h = self.text_rows as i32;
        if x < 0 || x >= w * 2 || y < 0 || y >= h * 2 {
            return 0;
        }
        let idx = (x >> 1) as usize + (y >> 1) as usize * w as usize;
        let mut p = self.vram_mut()[idx];
        let bit = (x & 1) as u32 + ((y & 1) as u32) * 2;
        if cmd == 3 {
            // POINT: ピクセル状態の読み出し。
            // 既に文字が描かれているセルは「全 ON」として扱う (= 0x8F)。
            if !PCG_PIXEL_RANGE.contains(&p) && p != 0 {
                p = PCG_PIXEL_BASE + 15;
            }
            return ((p as u32) & (1u32 << bit)) >> bit;
        }
        if !PCG_PIXEL_RANGE.contains(&p) {
            p = PCG_PIXEL_BASE;
        }
        match cmd {
            0 => p &= !(1u8 << bit),
            1 => p |= 1u8 << bit,
            2 => p ^= 1u8 << bit,
            _ => {}
        }
        self.vram_mut()[idx] = p;
        0
    }

    pub fn screen_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, cmd: i32) {
        let dx = x2 - x1;
        let dx2 = dx.abs();
        let dy = y2 - y1;
        let dy2 = dy.abs();
        if dx == 0 && dy == 0 {
            self.screen_pset(x1, y1, cmd);
            return;
        }
        if dx2 < dy2 {
            if y1 < y2 {
                let mut i = 0;
                while y1 + i <= y2 {
                    self.screen_pset(x1 + calc_div(dx * i, dy), y1 + i, cmd);
                    i += 1;
                }
            } else {
                let mut i = 0;
                while y2 + i <= y1 {
                    self.screen_pset(x2 + calc_div(dx * i, dy), y2 + i, cmd);
                    i += 1;
                }
            }
        } else if x1 < x2 {
            let mut i = 0;
            while x1 + i <= x2 {
                self.screen_pset(x1 + i, y1 + calc_div(dy * i, dx), cmd);
                i += 1;
            }
        } else {
            let mut i = 0;
            while x2 + i <= x1 {
                self.screen_pset(x2 + i, y2 + calc_div(dy * i, dx), cmd);
                i += 1;
            }
        }
    }
}
