// Copyright 2014-2024 the IchigoJam authors. All rights reserved. MIT license.
// https://github.com/IchigoJam/ichigojam-firm/blob/main/IchigoJam_BASIC/ram.h

//! IchigoJam の RAM レイアウト定義 (ram.h より移植)
//!
//! 仮想アドレス空間:
//! ```text
//! #000-#6FF  キャラクターパターン (ROM, font の先頭 224 文字)
//! #700-#7FF  PCG (RAM)
//! #800-#8FF  VAR (配列 + 変数 A-Z)
//! #900-#BFF  VRAM (32x24)
//! #C00-#1001 LIST (プログラム 1026 byte)
//! #1002-#1081 KEYBUF (128 byte)
//! #1082-#1149 LINEBUF (200 byte)
//! #114A-#117F I2CBUF (54 byte)
//! #1200-#19FF NET (ichigocrate 独自, 2KB: GET / POST の応答の書き込み先)
//! ```
//!
//! `#1180-#11FF` は使わない (NET 領域を覚えやすい `#1200` から始めるための隙間)。

#![allow(dead_code)]

pub const SIZE_PCG: usize = 32;
pub const N_LINEBUF: usize = 200;

pub const SIZE_RAM_PCG: usize = SIZE_PCG * 8;
pub const SIZE_RAM_VAR: usize = (102 + 26) * 2;
pub const SIZE_RAM_VRAM: usize = 32 * 24;
pub const SIZE_RAM_LIST: usize = 1024 + 2;
pub const SIZE_RAM_KEYBUF: usize = 128;
pub const SIZE_RAM_LINEBUF: usize = N_LINEBUF;
pub const SIZE_RAM_I2CBUF: usize = 54;

/// NET 領域 (ichigocrate 独自) の大きさ。応答の上限 (`net::MAX_RESPONSE_BYTES`) と同じ。
pub const SIZE_RAM_NET: usize = 2048;
/// NET 領域の仮想アドレス。
pub const ADDRESS_NET: usize = 0x1200;

/// RAM 全体の大きさ (元ファームの RAM + 隙間 + NET 領域)。
pub const SIZE_RAM: usize = OFFSET_RAM_NET + SIZE_RAM_NET;

pub const OFFSET_RAM_PCG: usize = 0;
pub const OFFSET_RAM_VAR: usize = OFFSET_RAM_PCG + SIZE_RAM_PCG;
pub const OFFSET_RAM_VRAM: usize = OFFSET_RAM_VAR + SIZE_RAM_VAR;
pub const OFFSET_RAM_LIST: usize = OFFSET_RAM_VRAM + SIZE_RAM_VRAM;
pub const OFFSET_RAM_KEYBUF: usize = OFFSET_RAM_LIST + SIZE_RAM_LIST;
pub const OFFSET_RAM_LINEBUF: usize = OFFSET_RAM_KEYBUF + SIZE_RAM_KEYBUF;
pub const OFFSET_RAM_I2CBUF: usize = OFFSET_RAM_LINEBUF + SIZE_RAM_LINEBUF;
pub const OFFSET_RAM_NET: usize = ADDRESS_NET - OFFSET_RAMROM;
const _: () = assert!(OFFSET_RAM_I2CBUF + SIZE_RAM_I2CBUF <= OFFSET_RAM_NET);

/// 仮想アドレス空間における RAM 開始オフセット (= 0x700)
pub const OFFSET_RAMROM: usize = (0x100 - SIZE_PCG) * 8;

pub const SCREEN_W: usize = 32;
pub const SCREEN_H: usize = 24;

/// 変数領域のサイズ
pub const IJB_SIZEOF_VAR: usize = 26; // A-Z
pub const IJB_SIZEOF_ARRAY: usize = 102;
pub const IJB_SIZEOF_GOSUB_STACK: usize = 30;
pub const IJB_SIZEOF_FOR_STACK: usize = 6;
pub const IJB_SIZEOF_LIST: usize = SIZE_RAM_LIST;
