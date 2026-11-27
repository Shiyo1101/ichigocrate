//! 貼り付け ([`prepare_paste`] + [`Session::queue_paste`]) のテスト。
//! 貼り付けたプログラムが、打ち込んだのと同じように登録・実行されること。

mod common;

use common::{screen_text, vram_line};
use ichigocrate_core::paste::prepare_paste;
use ichigocrate_core::session::{Session, FRAME_MS};
use ichigocrate_core::Machine;

fn paste(s: &mut Session, text: &str) {
    s.queue_paste(&prepare_paste(text).bytes);
}

/// tick を進めて、流し込みと実行が終わるまで待つ。
fn settle(s: &mut Session, t: &mut f64) {
    for _ in 0..600 {
        *t += FRAME_MS;
        let _ = s.tick(*t);
        if !s.is_running() && !s.has_pending_input() {
            return;
        }
    }
    panic!("終わらない\n{}", screen_text(&s.machine));
}

#[test]
fn pasted_program_is_registered_like_typed_lines() {
    let mut s = Session::new(Machine::new());
    let mut t = 0.0;
    paste(&mut s, "10 CLS\r\n20 ?\"HELLO\";1+2\r\n\r\n30 END\r\n");
    assert!(s.has_pending_input());
    settle(&mut s, &mut t);
    assert!(!s.has_pending_input());

    let _ = s.exec_line(b"RUN");
    settle(&mut s, &mut t);
    assert_eq!(vram_line(&s.machine, 0).trim_end(), "HELLO3");
}

/// 先に NEW を付ければ、前のプログラムの行は残らない。
#[test]
fn new_first_replaces_previous_program() {
    let mut s = Session::new(Machine::new());
    let mut t = 0.0;
    let _ = s.exec_line(b"99 ?\"OLD\"");
    s.queue_paste(&[b"NEW\n".as_slice(), &prepare_paste("10 ?\"NEW\"").bytes].concat());
    settle(&mut s, &mut t);
    let _ = s.exec_line(b"CLS:LIST");
    let screen = screen_text(&s.machine);
    assert!(screen.contains("10 ?\"NEW\""), "{screen}");
    assert!(!screen.contains("99 "), "{screen}");
}

/// 行番号の無い行はその場で実行される。ESC で残りの流し込みを取り消せる。
#[test]
fn immediate_lines_run_and_escape_cancels_rest() {
    let mut s = Session::new(Machine::new());
    let mut t = 0.0;
    paste(&mut s, "CLS:?\"NOW\"\n10 ?1\n");
    settle(&mut s, &mut t);
    assert!(screen_text(&s.machine).contains("NOW"));

    paste(&mut s, "20 ?2\n30 ?3\n");
    s.on_escape();
    assert!(!s.has_pending_input());
    settle(&mut s, &mut t);
    let _ = s.exec_line(b"CLS:LIST");
    let screen = screen_text(&s.machine);
    assert!(
        screen.contains("10 ?1") && !screen.contains("20 ?2"),
        "{screen}"
    );
}

/// 実行中に貼り付けても、止まって REPL に戻ってから打ち込まれる。
#[test]
fn paste_waits_until_program_stops() {
    let mut s = Session::new(Machine::new());
    let mut t = 0.0;
    let _ = s.exec_line(b"10 WAIT 30");
    let _ = s.exec_line(b"RUN");
    paste(&mut s, "20 ?\"LATER\"");
    t += FRAME_MS;
    let _ = s.tick(t);
    assert!(
        s.is_running() && s.has_pending_input(),
        "実行中は流し込まない"
    );
    settle(&mut s, &mut t);
    let _ = s.exec_line(b"CLS:LIST");
    assert!(screen_text(&s.machine).contains("20 ?\"LATER\""));
}

/// サンプル集のプログラムも貼り付けで丸ごと登録できる。
#[test]
fn sample_program_can_be_pasted() {
    let source = include_str!("../../samples/net-oxo.bas");
    let mut s = Session::new(Machine::new());
    let mut t = 0.0;
    paste(&mut s, source);
    settle(&mut s, &mut t);
    let lines = source.lines().filter(|l| !l.trim().is_empty()).count();
    let _ = s.exec_line(b"CLS:?FREE()");
    let free: i32 = vram_line(&s.machine, 0).trim().parse().unwrap();
    assert!(free > 0 && free < 200, "FREE={free}");
    assert_eq!(prepare_paste(source).numbered_lines, lines);
}
