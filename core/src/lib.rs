//! ichigojam-core: IchigoJam BASIC を Rust に移植した仮想マシン本体。
//!
//! - 言語: 日本語フォント版 (LANG_JP)
//! - バージョン: 1.4.3 ベース
//! - 非対応: IoT 拡張、Morse 拡張、多言語フォント、FLASH 保存

#![deny(unsafe_code)]

pub mod basic;
pub mod errors;
pub mod font;
pub mod keycodes;
pub mod keymap;
pub mod machine;
pub mod psg;
pub mod ram;
pub mod render;
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
    /// `INPUT` 文が対話入力待ちに入った。ホストは 1 行入力を受け取り
    /// [`Machine::input_complete`] を呼んだうえで [`Machine::basic_step`] を
    /// 呼び続けて実行を再開する。
    AwaitingInput,
}

/// REPL: 入力された 1 行を生バイト列として実行する。
///
/// `line` は IchigoJam の文字コード (ASCII 0x00-0x7F + グラフィック文字
/// 0x80-0xFF) のバイト列。RAM_LINEBUF にコピーした上で `basic_execute` を
/// 呼び出す。RUN や GOTO 等で実行が LIST 領域に移った場合は
/// `Ok(LineOutcome::Executed)` で即返るので、必要なら毎フレーム
/// [`Machine::basic_step`] を呼び続ける。
///
/// VRAM から読んだ生バイトをそのまま渡せるよう `&[u8]` を受ける。Rust の
/// `String` 経由 (`String::push(c as char)` → `as_bytes()`) は 0x80-0xFF を
/// UTF-8 で展開してしまうため、グラフィック文字を含む行はこの API を使う。
pub fn exec_line_bytes(machine: &mut Machine, line: &[u8]) -> Result<LineOutcome, BasicError> {
    let max = N_LINEBUF.saturating_sub(1);
    let n = line.len().min(max);
    machine.ram[OFFSET_RAM_LINEBUF..OFFSET_RAM_LINEBUF + n].copy_from_slice(&line[..n]);
    machine.ram[OFFSET_RAM_LINEBUF + n] = 0;
    match machine.basic_execute(OFFSET_RAM_LINEBUF) {
        BasicResult::Execute => Ok(LineOutcome::Executed),
        BasicResult::Edit => Ok(LineOutcome::Edited),
        BasicResult::Input => Ok(LineOutcome::AwaitingInput),
        BasicResult::StopOrErr => Err(machine.last_error.unwrap_or(BasicError::Break)),
    }
}

/// REPL: 入力された 1 行を ASCII 文字列として実行する。
///
/// 内部で [`exec_line_bytes`] を呼ぶ薄いラッパ。`line` に非 ASCII 文字
/// (`char as u32 >= 0x80`) を含めると `as_bytes()` が UTF-8 へ展開するため
/// 0x80-0xFF のグラフィック文字は保持されない。生バイトを渡したいときは
/// [`exec_line_bytes`] を使うこと。
pub fn exec_line(machine: &mut Machine, line: &str) -> Result<LineOutcome, BasicError> {
    exec_line_bytes(machine, line.as_bytes())
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
