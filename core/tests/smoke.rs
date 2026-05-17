//! 最小限の動作確認テスト

use ichigojam_core::{exec_line, run_to_completion, Machine, OFFSET_RAM_VRAM, SCREEN_W};

fn screen_text(m: &Machine) -> String {
    let mut s = String::new();
    let v = &m.ram[OFFSET_RAM_VRAM..OFFSET_RAM_VRAM + 32 * 24];
    for (i, c) in v.iter().enumerate() {
        if i > 0 && i % SCREEN_W == 0 {
            s.push('\n');
        }
        if *c == 0 {
            s.push(' ');
        } else if *c >= 32 && *c < 127 {
            s.push(*c as char);
        } else {
            s.push('?');
        }
    }
    s
}

fn vram_line(m: &Machine, y: usize) -> String {
    let v = &m.ram[OFFSET_RAM_VRAM + y * SCREEN_W..OFFSET_RAM_VRAM + (y + 1) * SCREEN_W];
    let mut s = String::new();
    for c in v {
        if *c == 0 {
            break;
        }
        s.push(*c as char);
    }
    s
}

#[test]
fn print_simple_expression() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "?1+2");
    assert_eq!(vram_line(&m, 0), "3");
}

#[test]
fn print_with_string() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "?\"HI\"");
    assert_eq!(vram_line(&m, 0), "HI");
}

#[test]
fn variable_assignment() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "A=42");
    let _ = exec_line(&mut m, "?A");
    assert!(vram_line(&m, 0).contains("42"), "got: {:?}", screen_text(&m));
}

#[test]
fn line_edit_and_list() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 ?\"X\"");
    let _ = exec_line(&mut m, "20 ?\"Y\"");
    let _ = exec_line(&mut m, "LIST");
    let t = screen_text(&m);
    assert!(t.contains("10 ?\"X\""), "LIST output missing line 10:\n{t}");
    assert!(t.contains("20 ?\"Y\""), "LIST output missing line 20:\n{t}");
}

#[test]
fn for_next_loop() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 FOR I=1 TO 3");
    let _ = exec_line(&mut m, "20 ?I");
    let _ = exec_line(&mut m, "30 NEXT");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    let t = screen_text(&m);
    assert!(t.contains("1"), "{t}");
    assert!(t.contains("2"), "{t}");
    assert!(t.contains("3"), "{t}");
}

#[test]
fn goto_and_if() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 A=5");
    let _ = exec_line(&mut m, "20 IF A=5 GOTO 40");
    let _ = exec_line(&mut m, "30 ?\"NO\"");
    let _ = exec_line(&mut m, "40 ?\"YES\"");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    let t = screen_text(&m);
    assert!(t.contains("YES"), "{t}");
    assert!(!t.contains("NO"), "{t}");
}

#[test]
fn cls_clears_vram() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "?\"HELLO\"");
    let _ = exec_line(&mut m, "CLS");
    let t = screen_text(&m);
    assert!(!t.contains("HELLO"), "{t}");
}

#[test]
fn hex_and_bin() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "?#ff");
    let _ = exec_line(&mut m, "?`101");
    let t = screen_text(&m);
    assert!(t.contains("255"), "{t}");
    assert!(t.contains("5"), "{t}");
}
