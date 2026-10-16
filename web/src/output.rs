//! 画面 (VRAM) を文字列として取り出す処理と、onPrint (画面出力ストリーミング) の
//! 差分抽出。
//!
//! core を一切改変せず、フレーム間の VRAM スナップショット差分で PRINT 出力を
//! 近似する。canvas や JS に依存しないのでネイティブで単体テストできる。

use ichigocrate_core::Machine;

/// VRAM の 1 バイトを onPrint 用の文字へ変換する (印字不能・グラフィックは `?`)。
fn screen_char(c: u8) -> char {
    match c {
        0 => ' ',
        32..=126 => c as char,
        _ => '?',
    }
}

fn cells_to_string(cells: &[u8]) -> String {
    cells.iter().map(|&c| screen_char(c)).collect()
}

/// 画面全体を、各行の末尾空白を詰めて改行で連結した文字列にする。
pub(crate) fn screen_text(machine: &Machine) -> String {
    let cols = machine.screen_cols();
    machine
        .vram()
        .chunks(cols)
        .take(machine.screen_rows())
        .map(|row| cells_to_string(row).trim_end().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 直前フレームからのスクロール量 (行数) を検出する。`cur` の上側 (rows-k) 行が
/// `prev` の下側に一致する最小の k を返す。一致しなければ 0。1 フレーム内に複数行
/// スクロールしてもここで検出できる範囲は補正する (画面外へ流れ切った分は不可)。
fn detect_scroll(prev: &[u8], cur: &[u8], cols: usize, rows: usize) -> usize {
    if prev.len() != cur.len() || cols == 0 || rows == 0 {
        return 0;
    }
    (1..rows)
        .find(|&k| {
            let n = (rows - k) * cols;
            cur[..n] == prev[k * cols..k * cols + n]
        })
        .unwrap_or(0)
}

/// 前回フレームまでに onPrint へ流した位置を覚え、新しく画面へ出た文字だけを取り出す。
#[derive(Debug, Default)]
pub(crate) struct OutputTracker {
    /// 直前フレームの VRAM スナップショット (スクロール検出用)。
    prev_vram: Vec<u8>,
    /// 出力済みの位置 (列, 行)。ここから現在のカーソルまでが新規出力。
    emitted_x: usize,
    emitted_y: usize,
}

impl OutputTracker {
    /// 現在の画面を出力済みとして差分の基準にする。起動バナーを流さないよう、
    /// 起動直後とリセット直後に呼ぶ。
    pub(crate) fn mark_emitted(&mut self, machine: &Machine) {
        self.prev_vram.clear();
        self.prev_vram.extend_from_slice(machine.vram());
        (self.emitted_x, self.emitted_y) = cursor_cell(machine);
    }

    /// 前回呼び出しから新しく出力された文字列を返す (無ければ空文字列)。
    ///
    /// VRAM のスクロール量を補正したうえで、出力済み位置から現在のカーソルまでを
    /// 行単位で取り出す。LOCATE 等でカーソルを戻して上書きした出力は拾えない。
    pub(crate) fn take_new_output(&mut self, machine: &Machine) -> String {
        let cols = machine.screen_cols();
        let rows = machine.screen_rows();
        let vram = machine.vram();
        let (cx, cy) = cursor_cell(machine);

        let scroll = detect_scroll(&self.prev_vram, vram, cols, rows);
        self.emitted_y = self.emitted_y.saturating_sub(scroll);

        let mut chunk = String::new();
        if (self.emitted_y, self.emitted_x) < (cy, cx) {
            for y in self.emitted_y..=cy {
                let start = if y == self.emitted_y {
                    self.emitted_x
                } else {
                    0
                };
                let end = if y == cy { cx } else { cols };
                let line = cells_to_string(&vram[y * cols + start..y * cols + end]);
                if y < cy {
                    // 折返しで満杯の行は詰まらないので、行末空白の除去で区別できる。
                    chunk.push_str(line.trim_end());
                    chunk.push('\n');
                } else {
                    chunk.push_str(&line);
                }
            }
        }

        self.emitted_x = cx;
        self.emitted_y = cy;
        self.prev_vram.clear();
        self.prev_vram.extend_from_slice(vram);
        chunk
    }
}

/// カーソル位置を VRAM の有効範囲へ丸めた (列, 行)。列は行末 (= cols) を許す。
fn cursor_cell(machine: &Machine) -> (usize, usize) {
    let cols = machine.screen_cols();
    let rows = machine.screen_rows();
    let x = (machine.cursorx.max(0) as usize).min(cols);
    let y = (machine.cursory.max(0) as usize).min(rows.saturating_sub(1));
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ichigocrate_core::session::Session;

    #[test]
    fn detect_scroll_finds_single_and_multi_line_shift() {
        let cols = 4;
        let rows = 3;
        // prev: 行 0=AAAA 1=BBBB 2=CCCC
        let prev = b"AAAABBBBCCCC".to_vec();
        // 1 行スクロール: 上が BBBB CCCC に詰まり末尾が新規。
        let mut cur = b"BBBBCCCC????".to_vec();
        assert_eq!(detect_scroll(&prev, &cur, cols, rows), 1);
        // スクロールなし。
        assert_eq!(detect_scroll(&prev, &prev, cols, rows), 0);
        // 2 行スクロール。
        cur = b"CCCC????????".to_vec();
        assert_eq!(detect_scroll(&prev, &cur, cols, rows), 2);
    }

    #[test]
    fn tracker_skips_banner_and_streams_print_output() {
        let mut session = Session::new(Machine::new());
        let mut tracker = OutputTracker::default();
        tracker.mark_emitted(&session.machine);
        assert_eq!(tracker.take_new_output(&session.machine), "");

        session.exec_line(b"PRINT 1+2");
        assert_eq!(tracker.take_new_output(&session.machine), "3\nOK\n");
        assert_eq!(tracker.take_new_output(&session.machine), "");
    }

    #[test]
    fn screen_text_trims_trailing_spaces_per_row() {
        let mut session = Session::new(Machine::new());
        session.exec_line(b"CLS:PRINT \"HI\"");
        let text = screen_text(&session.machine);
        assert!(text.starts_with("HI\nOK\n\n"));
        assert_eq!(text.split('\n').count(), session.machine.screen_rows());
    }
}
