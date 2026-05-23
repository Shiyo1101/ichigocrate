//! ichigojam-core: IchigoJam BASIC を Rust に移植した仮想マシン本体。
//!
//! - 言語: 日本語フォント版 (LANG_JP)
//! - バージョン: 1.4.3 ベース
//! - 非対応: IoT 拡張、Morse 拡張、多言語フォント、FLASH 保存

pub mod basic;
pub mod errors;
pub mod font;
pub mod keycodes;
pub mod machine;
pub mod psg;
pub mod ram;
pub mod romajikana;
pub mod screen;
pub mod tokens;

pub use errors::BasicError;
pub use machine::{BasicResult, Machine, Token, PC_NULL};
pub use ram::{
    N_LINEBUF, OFFSET_RAMROM, OFFSET_RAM_LINEBUF, OFFSET_RAM_LIST, OFFSET_RAM_VRAM, SCREEN_H,
    SCREEN_W, SIZE_RAM, SIZE_RAM_LINEBUF, SIZE_RAM_VRAM,
};

/// `exec_line` の成功時の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOutcome {
    /// 1 行を実行完了。REPL は次に `OK` を表示すべき。
    Executed,
    /// 行編集 (LIST に対する追加・削除)。`OK` は表示しない。
    Edited,
}

/// REPL: 入力された 1 行を実行する。
///
/// `line` は ASCII 文字列。RAM_LINEBUF にコピーした上で `basic_execute`
/// を呼び出す。RUN や GOTO 等で実行が LIST 領域に移った場合は
/// `Ok(LineOutcome::Executed)` で即返るので、必要なら毎フレーム
/// [`Machine::basic_step`] を呼び続ける。
pub fn exec_line(machine: &mut Machine, line: &str) -> Result<LineOutcome, BasicError> {
    let bytes = line.as_bytes();
    let max = N_LINEBUF.saturating_sub(1);
    let n = bytes.len().min(max);
    for (i, &b) in bytes.iter().take(n).enumerate() {
        machine.ram[OFFSET_RAM_LINEBUF + i] = b;
    }
    machine.ram[OFFSET_RAM_LINEBUF + n] = 0;
    match machine.basic_execute(OFFSET_RAM_LINEBUF) {
        BasicResult::Execute => Ok(LineOutcome::Executed),
        BasicResult::Edit => Ok(LineOutcome::Edited),
        // 停止理由は basic_step が last_error に記録済み。記録が無いケースは
        // 現状想定しないが、念のため Break として扱う。
        BasicResult::StopOrErr => Err(machine.last_error.unwrap_or(BasicError::Break)),
    }
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
