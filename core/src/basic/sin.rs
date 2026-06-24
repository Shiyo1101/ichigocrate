// Copyright 2014-2024 the IchigoJam authors. All rights reserved. MIT license.
// https://github.com/IchigoJam/ichigojam-firm/blob/main/IchigoJam_BASIC/basic.h

//! 整数 sin テーブル。
//!
//! 入力は度数。出力は -256..=256。0 度は厳密に 0 を返す。

const SIN_TABLE: [u8; 91] = [
    0, 3, 8, 12, 17, 21, 26, 30, 35, 39, 43, 48, 52, 57, 61, 65, 70, 74, 78, 82, 87, 91, 95, 99,
    103, 107, 111, 115, 119, 123, 127, 131, 135, 138, 142, 146, 149, 153, 157, 160, 164, 167, 170,
    174, 177, 180, 183, 186, 189, 192, 195, 198, 201, 203, 206, 209, 211, 214, 216, 218, 221, 223,
    225, 227, 229, 231, 233, 235, 236, 238, 240, 241, 242, 244, 245, 246, 247, 248, 249, 250, 251,
    252, 253, 253, 254, 254, 254, 255, 255, 255, 255,
];

pub fn sin360(mut deg: i32) -> i32 {
    let mut pm = 1;
    if deg < 0 {
        deg = -deg;
        pm = -pm;
    }
    while deg > 360 {
        deg -= 360;
    }
    if deg > 180 {
        deg -= 180;
        pm = -pm;
    }
    if deg > 90 {
        deg = 180 - deg;
    }
    if deg == 0 {
        return 0;
    }
    pm * (SIN_TABLE[deg as usize] as i32 + 1)
}
