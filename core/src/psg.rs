// Copyright 2014-2024 the IchigoJam authors. All rights reserved. MIT license.
// https://github.com/IchigoJam/ichigojam-firm/blob/main/IchigoJam_BASIC/psg.h

//! PSG: MML プレイヤと BEEP。
//!
//! 元実装はハードウェアタイマでの周期トグルだが、本ポートはデスクトップ
//! 用途のため `current_tone_hz` を周波数 (Hz) で公開し、音声バックエンド側
//! が矩形波を生成する。

use crate::machine::{basic_toupper, Machine};
use crate::ram::OFFSET_RAMROM;

pub const PSG_TICK_FREQ: u32 = 60; // 毎フレーム1回
pub const PSG_TICK_PER_SEC: u32 = 60;
pub const PSG_DEFAULT_OCT: u8 = 3;
pub const PSG_DEFAULT_LEN: u8 = 8;
pub const PSG_DEFAULT_TEMPO: u16 = 120;

/// 音階インデックス `t` (0=O1C, 1=O1C#, ...) を Hz に変換。
/// t=24 が C4 (MIDI 60, 261.63 Hz) になるよう揃えている。
fn t_to_hz(t: i32) -> f32 {
    let midi = 36.0 + t as f32;
    440.0 * (2.0f32).powf((midi - 69.0) / 12.0)
}

impl Machine {
    /// MML 文字列をセットして再生開始。`mml_addr` は仮想アドレス。
    pub fn psg_play_mml(&mut self, mml_addr: Option<i32>) {
        self.psg_mml_pos = mml_addr.and_then(|a| {
            if a >= OFFSET_RAMROM as i32 {
                Some((a - OFFSET_RAMROM as i32) as usize)
            } else {
                None
            }
        });
        self.psg_octave = PSG_DEFAULT_OCT;
        self.psg_default_note_len = PSG_DEFAULT_LEN;
        self.psg_tempo_bpm = PSG_DEFAULT_TEMPO;
        self.psg_tone = 0;
        self.psg_remaining_frames = 0;
        self.psg_repeat_pos = None;
        if self.psg_mml_pos.is_none() {
            self.current_tone_hz = 0.0;
        }
    }

    /// 指定トーン (TONE[] 配列上のインデックス) と長さ (frames) でビープ
    pub fn psg_beep(&mut self, tone: i16, len: i16) {
        let ratio = self.psg_tick_ratio.max(1) as u32;
        self.psg_remaining_frames =
            (len as u32) * (PSG_TICK_FREQ / PSG_TICK_PER_SEC) * ratio;

        self.psg_tone = (tone as u32 * ratio) as u16;
        self.psg_mml_pos = None;
        // tone 値を Hz に変換 (簡易: 8000 / tone を擬似的に)
        self.current_tone_hz = if tone > 0 {
            8000.0 / tone as f32
        } else {
            0.0
        };
    }

    pub fn psg_tempo(&mut self, tempo: i16) {
        self.psg_tempo_bpm = tempo as u16;
    }

    pub fn psg_sound(&self) -> bool {
        self.psg_tone != 0 || self.psg_mml_pos.is_some()
    }

    /// 60Hz tick — MML を進める
    pub fn psg_tick(&mut self) {
        if self.psg_remaining_frames > 0 {
            self.psg_remaining_frames -= 1;
            if self.psg_remaining_frames > 0 {
                return;
            }
            if self.psg_mml_pos.is_none() {
                self.psg_tone = 0;
                self.current_tone_hz = 0.0;
            }
        }
        if self.psg_mml_pos.is_none() {
            return;
        }

        let mut flg = false;
        loop {
            let mut t: i32 = -2;
            let mut s: i32 = 0;
            let c = basic_toupper(self.mml_next());
            match c {
                b'<' => {
                    self.psg_octave = self.psg_octave.wrapping_add(1);
                    continue;
                }
                b'>' => {
                    self.psg_octave = self.psg_octave.wrapping_sub(1);
                    continue;
                }
                b'O' => {
                    let c2 = self.mml_peek();
                    if c2 == 0 {
                        continue;
                    }
                    self.mml_advance();
                    self.psg_octave = (c2 as i32 - b'0' as i32) as u8;
                    continue;
                }
                b' ' => continue,
                b'L' => {
                    let l = self.mml_parse_int();
                    if l > 0 {
                        self.psg_default_note_len = (32 / l) as u8;
                        let c3 = self.mml_peek();
                        if c3 == b'.' {
                            self.mml_advance();
                            self.psg_default_note_len += self.psg_default_note_len >> 1;
                        }
                    }
                    continue;
                }
                b'T' => {
                    let v = self.mml_parse_int();
                    self.psg_tempo_bpm = v as u16;
                    continue;
                }
                b'N' => {
                    s = self.mml_parse_int() as i32;
                    if s > 255 {
                        s = 255;
                    }
                }
                b'C' => t = 0,
                b'D' => t = 2,
                b'E' => t = 4,
                b'F' => t = 5,
                b'G' => t = 7,
                b'A' => t = 9,
                b'B' => t = 11,
                b'R' => t = -2,
                b'$' => {
                    if self.mml_peek() != 0 {
                        self.psg_repeat_pos = self.psg_mml_pos;
                    }
                    continue;
                }
                _ => {
                    if self.psg_repeat_pos.is_some() && !flg {
                        self.psg_mml_pos = self.psg_repeat_pos;
                        flg = true;
                        continue;
                    }
                    self.psg_mml_pos = None;
                    self.psg_tone = 0;
                    self.psg_remaining_frames = 0;
                    self.current_tone_hz = 0.0;
                    return;
                }
            }

            let c2 = self.mml_peek();
            if c2 == b'+' || c2 == b'#' {
                self.mml_advance();
                if t != 11 {
                    t += 1;
                }
            } else if c2 == b'-' {
                self.mml_advance();
                if t > 0 {
                    t -= 1;
                }
            }
            let mut len = self.mml_parse_int();
            if len > 0 {
                len = 32 / len;
            } else {
                len = self.psg_default_note_len as u32;
            }
            let c4 = self.mml_peek();
            if c4 == b'.' {
                self.mml_advance();
                len += len >> 1;
            }
            if t > -2 {
                t += (self.psg_octave as i32 - 1) * 12;
            }

            // 音高 → Hz (ratio や TONE テーブルは使わず音楽的に正しい周波数)
            if s == 0 {
                if t >= 0 {
                    self.current_tone_hz = t_to_hz(t);
                    self.psg_tone = 1; // 鳴っているマーク (非ゼロ)
                } else if t == -2 {
                    self.current_tone_hz = 0.0;
                    self.psg_tone = 0;
                }
            } else {
                self.current_tone_hz = s as f32 * 8.0;
                self.psg_tone = 1;
            }

            let ratio = self.psg_tick_ratio.max(1) as u32;
            let tempo = self.psg_tempo_bpm.max(1) as u32;
            self.psg_remaining_frames = len * ((60 * PSG_TICK_FREQ) >> 3) * ratio / tempo;
            break;
        }
    }

    fn mml_peek(&self) -> u8 {
        match self.psg_mml_pos {
            Some(p) if p < self.ram.len() => self.ram[p],
            _ => 0,
        }
    }

    fn mml_next(&mut self) -> u8 {
        match self.psg_mml_pos {
            Some(p) if p < self.ram.len() => {
                let c = self.ram[p];
                self.psg_mml_pos = Some(p + 1);
                c
            }
            _ => 0,
        }
    }

    fn mml_advance(&mut self) {
        if let Some(p) = self.psg_mml_pos {
            self.psg_mml_pos = Some(p + 1);
        }
    }

    fn mml_parse_int(&mut self) -> u32 {
        while self.mml_peek() == b' ' {
            self.mml_advance();
        }
        let mut a: u32 = 0;
        let mut any = false;
        loop {
            let c = self.mml_peek();
            if !c.is_ascii_digit() {
                break;
            }
            a = a * 10 + (c - b'0') as u32;
            self.mml_advance();
            any = true;
        }
        if !any {
            0
        } else {
            a
        }
    }
}
