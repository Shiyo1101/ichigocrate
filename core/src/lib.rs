//! ichigojam-core: IchigoJam BASIC を Rust に移植した仮想マシン本体。
//!
//! - 言語: 日本語フォント版 (LANG_JP)
//! - バージョン: 1.4.3 ベース
//! - 非対応: IoT 拡張、Morse 拡張、ローマ字かな変換、多言語フォント、FLASH 保存

pub mod basic;
pub mod errors;
pub mod font;
pub mod machine;
pub mod psg;
pub mod ram;
pub mod screen;
pub mod tokens;

pub use machine::{BasicResult, Machine, Token, PC_NULL};
pub use ram::{
    N_LINEBUF, OFFSET_RAMROM, OFFSET_RAM_LINEBUF, OFFSET_RAM_LIST, OFFSET_RAM_VRAM, SCREEN_H,
    SCREEN_W, SIZE_RAM, SIZE_RAM_LINEBUF, SIZE_RAM_VRAM,
};

/// REPL: 入力された 1 行を実行する。
///
/// `line` は ASCII 文字列。RAM_LINEBUF にコピーした上で `basic_execute`
/// を呼び出す。RUN や GOTO 等で実行が LIST 領域に移った場合は呼出元へ
/// 即返るので、必要なら `run_to_completion` で続きを駆動する。
pub fn exec_line(machine: &mut Machine, line: &str) -> BasicResult {
    let bytes = line.as_bytes();
    let max = N_LINEBUF.saturating_sub(1);
    let n = bytes.len().min(max);
    for (i, &b) in bytes.iter().take(n).enumerate() {
        machine.ram[OFFSET_RAM_LINEBUF + i] = b;
    }
    machine.ram[OFFSET_RAM_LINEBUF + n] = 0;
    machine.basic_execute(OFFSET_RAM_LINEBUF)
}

/// テスト/ヘッドレス用: プログラム実行を同期的に最後まで進める。
/// `wait_frames` は無視され (即時進行)、ESC 押下や PC == NULL で終了。
pub fn run_to_completion(machine: &mut Machine) {
    while machine.pc != PC_NULL {
        machine.wait_frames = 0;
        if machine.basic_step().is_some() {
            break;
        }
        if machine.stop_execute() {
            break;
        }
    }
}
