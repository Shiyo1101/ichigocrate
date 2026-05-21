//! screen.h を Rust に移植。VRAM 文字操作およびピクセル描画。

use crate::font::CHAR_PATTERN_JP;
use crate::machine::{calc_div, Machine};
use crate::ram::*;

impl Machine {
    /// VRAM の生バッファ
    pub fn vram(&self) -> &[u8] {
        &self.ram[OFFSET_RAM_VRAM..OFFSET_RAM_VRAM + SIZE_RAM_VRAM]
    }

    pub fn vram_mut(&mut self) -> &mut [u8] {
        &mut self.ram[OFFSET_RAM_VRAM..OFFSET_RAM_VRAM + SIZE_RAM_VRAM]
    }

    pub fn pcg(&self) -> &[u8] {
        &self.ram[OFFSET_RAM_PCG..OFFSET_RAM_PCG + SIZE_RAM_PCG]
    }

    /// PCG をフォントの末尾 32 文字で初期化する (CLP)
    pub fn screen_clp(&mut self) {
        let src = &CHAR_PATTERN_JP[(0x100 - SIZE_PCG) * 8..(0x100 * 8)];
        self.ram[OFFSET_RAM_PCG..OFFSET_RAM_PCG + SIZE_RAM_PCG].copy_from_slice(src);
    }

    pub fn screen_clear(&mut self) {
        self.cursorx = 0;
        self.cursory = 0;
        for b in self.vram_mut() {
            *b = 0;
        }
    }

    pub fn video_clt(&mut self) {
        self.frames = 0;
        self.linecnt = 0;
    }

    pub fn video_tick(&self, n: i16) -> i16 {
        let v = if n != 0 { self.linecnt } else { self.frames };
        (v & 0x7fff) as i16
    }

    pub fn screen_get(&self, x: i32, y: i32) -> u8 {
        if x < 0 || x >= self.screenw as i32 || y < 0 || y >= self.screenh as i32 {
            return 0;
        }
        self.vram()[(y as usize) * self.screenw + x as usize]
    }

    pub fn screen_get_current(&self) -> u8 {
        self.screen_get(self.cursorx, self.cursory)
    }

    pub fn screen_locate(&mut self, mut x: i32, mut y: i32) {
        if x < 0 {
            x = 0;
        } else if x >= self.screenw as i32 {
            x = self.screenw as i32 - 1;
        }
        if y < 0 {
            y = -1;
        } else if y >= self.screenh as i32 {
            y = self.screenh as i32 - 1;
        }
        self.cursorx = x;
        self.cursory = y;
    }

    pub fn screen_scroll(&mut self, n: i32) {
        let w = self.screenw;
        let h = self.screenh;
        let v = self.vram_mut();
        let dir = match n {
            0 | 30 => 30,
            1 | 29 => 29,
            2 | 31 => 31,
            3 | 28 => 28,
            _ => return,
        };
        match dir {
            30 => {
                // UP
                v.copy_within(w..w * h, 0);
                for b in &mut v[(h - 1) * w..h * w] {
                    *b = 0;
                }
            }
            29 => {
                // RIGHT
                for i in (0..w - 1).rev() {
                    for j in 0..h {
                        v[j * w + (i + 1)] = v[j * w + i];
                    }
                }
                for j in 0..h {
                    v[j * w] = 0;
                }
            }
            31 => {
                // DOWN
                for i in (0..h - 1).rev() {
                    v.copy_within(i * w..(i + 1) * w, (i + 1) * w);
                }
                for b in &mut v[0..w] {
                    *b = 0;
                }
            }
            28 => {
                // LEFT
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
        if self.cursory == self.screenh as i32 - 1 {
            self.screen_scroll(0);
        } else {
            self.cursory += 1;
        }
        self.cursorx = 0;
    }

    /// 改行/カーソル/編集を含む 1 文字出力
    pub fn screen_putc(&mut self, c: u8) {
        if self.screen_locatemode != 0 {
            if self.screen_locatemode == 2 {
                if c < 32 {
                    self.screen_scroll(c as i32);
                    self.screen_locatemode -= 1;
                } else {
                    self.screen_locate(c as i32 - 32, self.cursory);
                }
            } else {
                self.screen_locate(self.cursorx, c as i32 - 32);
            }
            self.screen_locatemode = self.screen_locatemode.saturating_sub(1);
            return;
        }
        if self.cursory == -1 {
            return;
        }
        let w = self.screenw;
        let h = self.screenh;

        match c {
            b'\r' => {}
            b'\n' => {
                loop {
                    let cy = self.cursory;
                    let line_end = (cy as usize + 1) * w - 1;
                    let v = self.vram();
                    if v[line_end] == 0 {
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
            0x08 | 0x7f => {
                // backspace / delete
                if c == 0x7f {
                    // delete in place: skip cursor move
                } else if self.cursorx > 0 {
                    self.cursorx -= 1;
                } else if self.cursory > 0
                    && self.vram()[self.cursory as usize * w - 1] != 0
                {
                    self.cursory -= 1;
                    self.cursorx = w as i32 - 1;
                } else {
                    return;
                }
                let mut i = self.cursory as usize * w + self.cursorx as usize;
                while i < w * h - 1 {
                    let v = self.vram_mut();
                    if v[i] == 0 {
                        break;
                    }
                    v[i] = v[i + 1];
                    i += 1;
                }
                if i == w * h - 1 {
                    self.vram_mut()[i] = 0;
                }
            }
            28 => {
                // LEFT
                if self.cursorx > 0 {
                    self.cursorx -= 1;
                } else if self.cursory > 0
                    && (self.vram()[self.cursory as usize * w - 1] != 0
                        || self.screen_insertmode)
                {
                    self.cursory -= 1;
                    self.cursorx = w as i32 - 1;
                }
            }
            29 => {
                // RIGHT
                if (self.cursorx != w as i32 - 1 || self.cursory != h as i32 - 1)
                    && (self.vram()
                        [self.cursory as usize * w + self.cursorx as usize]
                        != 0
                        || self.screen_insertmode)
                    {
                        self.cursorx += 1;
                        if self.cursorx == w as i32 {
                            self.cursorx = 0;
                            self.cursory += 1;
                        }
                    }
            }
            30 => {
                // UP
                if self.cursory > 0 {
                    self.cursory -= 1;
                }
            }
            31 => {
                // DOWN
                if self.cursory < h as i32 - 1 {
                    self.cursory += 1;
                }
            }
            0x10 => {
                // 行分割。簡略化のため改行扱い
                self.screen_enter();
            }
            12 => {
                // FF: カーソル以降を消去
                let now = self.cursory as usize * w + self.cursorx as usize;
                let total = w * h;
                for b in &mut self.vram_mut()[now..total] {
                    *b = 0;
                }
            }
            21 => {
                self.screen_locatemode = 2;
            }
            15 => {
                self.key_kana = !self.key_kana;
            }
            17 => {
                self.key_insert = !self.key_insert;
            }
            _ => {
                if c < 32 && c != 0 {
                    // 制御コードは無視
                    return;
                }
                if !self.screen_insertmode {
                    // 挿入モード
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

    /// screen から現在カーソル行の論理行を取得 (Enter 押下時用)
    pub fn screen_gets(&mut self) -> usize {
        if self.cursory == -1 {
            self.cursory = 0;
            self.cursorx = 0;
        }
        let w = self.screenw;
        let p = (self.cursory as i64 - 1) * w as i64;
        if p < 0 {
            return OFFSET_RAM_VRAM;
        }
        let mut p = p as usize;
        // 元 C: ((!*p) && *(p-1)) なら p -= SCREEN_W
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

    // ============================================================
    // ピクセル描画 (DRAW / POINT)
    // ============================================================

    pub fn screen_pset(&mut self, x: i32, y: i32, cmd: i32) -> u32 {
        let w = self.screenw as i32;
        let h = self.screenh as i32;
        if x < 0 || x >= w * 2 || y < 0 || y >= h * 2 {
            return 0;
        }
        let idx = (x >> 1) as usize + (y >> 1) as usize * w as usize;
        let mut p = self.vram_mut()[idx];
        let n = (x & 1) as u32 + ((y & 1) as u32) * 2;
        if cmd == 3 {
            if !(128..128 + 16).contains(&p) && p != 0 {
                p = 128 + 15;
            }
            return ((p as u32) & (1u32 << n)) >> n;
        }
        if !(128..128 + 16).contains(&p) {
            p = 128;
        }
        match cmd {
            0 => p &= !(1u8 << n),
            1 => p |= 1u8 << n,
            2 => p ^= 1u8 << n,
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
