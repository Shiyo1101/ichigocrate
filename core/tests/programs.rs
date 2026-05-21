//! より複雑なプログラム例

use ichigojam_core::{exec_line, run_to_completion, Machine, OFFSET_RAM_VRAM, SCREEN_W};

fn screen(m: &Machine) -> String {
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
        }
    }
    s
}

#[test]
fn gosub_return() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 ?\"START\"");
    let _ = exec_line(&mut m, "20 GOSUB 100");
    let _ = exec_line(&mut m, "30 ?\"END\"");
    let _ = exec_line(&mut m, "40 END");
    let _ = exec_line(&mut m, "100 ?\"SUB\"");
    let _ = exec_line(&mut m, "110 RETURN");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    let t = screen(&m);
    assert!(t.contains("START"), "{t}");
    assert!(t.contains("SUB"), "{t}");
    assert!(t.contains("END"), "{t}");
}

#[test]
fn fib_program() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 A=0:B=1");
    let _ = exec_line(&mut m, "20 FOR I=1 TO 5");
    let _ = exec_line(&mut m, "30 ?B;\" \";");
    let _ = exec_line(&mut m, "40 C=A+B:A=B:B=C");
    let _ = exec_line(&mut m, "50 NEXT");
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    let t = screen(&m);
    // Fib: 1 1 2 3 5
    assert!(t.contains("1 1 2 3 5"), "{t}");
}

#[test]
fn nested_if_else() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "A=10:IF A>5 ?\"BIG\" ELSE ?\"SMALL\"");
    let t = screen(&m);
    assert!(t.contains("BIG"), "{t}");
}

#[test]
fn wait_and_goto_loop_yields() {
    // 元プログラム: 10 ?"ICHIGOJAM RS" / 20 WAIT60 / 30 GOTO10
    // RUN 後、basic_execute は LIST へ移行した時点で呼出元へ制御を返すこと、
    // WAIT で wait_frames がセットされて以降の自動進行が止まることを確認。
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 ?\"ICHIGOJAM RS\"");
    let _ = exec_line(&mut m, "20 WAIT60");
    let _ = exec_line(&mut m, "30 GOTO10");
    let _ = exec_line(&mut m, "RUN");
    // 1 周目: PRINT 実行 → WAIT で stop → wait_frames が 60
    while m.wait_frames == 0 && m.pc != ichigojam_core::PC_NULL {
        let _ = m.basic_step();
    }
    assert_eq!(m.wait_frames, 60);
    // 1 行目が出力されていること
    let t = screen(&m);
    assert!(t.contains("ICHIGOJAM RS"), "{t}");
}

#[test]
fn save_load_roundtrip() {
    use ichigojam_core::machine::Storage;
    #[derive(Debug)]
    struct MemStore { data: Vec<u8>, has: bool }
    impl Storage for MemStore {
        fn save(&mut self, _slot: u8, d: &[u8]) -> bool {
            self.data = d.to_vec();
            self.has = true;
            true
        }
        fn load(&mut self, _slot: u8, buf: &mut [u8]) -> Option<usize> {
            if !self.has { return None; }
            let n = self.data.len().min(buf.len());
            buf[..n].copy_from_slice(&self.data[..n]);
            buf[n..].fill(0);
            Some(n)
        }
        fn peek(&mut self, slot: u8, buf: &mut [u8]) -> Option<usize> {
            self.load(slot, buf)
        }
    }
    let mut m = Machine::new();
    m.set_storage(Box::new(MemStore { data: vec![], has: false }));
    let _ = exec_line(&mut m, "10 ?\"HELLO\"");
    let _ = exec_line(&mut m, "20 ?\"WORLD\"");
    let original_size = m.listsize;
    let _ = exec_line(&mut m, "SAVE 0");
    let _ = exec_line(&mut m, "NEW");
    assert_eq!(m.listsize, 0);
    let _ = exec_line(&mut m, "LOAD 0");
    assert_eq!(m.listsize, original_size);
    let _ = exec_line(&mut m, "RUN");
    run_to_completion(&mut m);
    let t = screen(&m);
    assert!(t.contains("HELLO"), "{t}");
    assert!(t.contains("WORLD"), "{t}");
}

#[test]
fn poke_peek_pcg() {
    let mut m = Machine::new();
    // PCG 領域 (0x700) に書き込んで読み戻し
    let _ = exec_line(&mut m, "POKE #700,#FF,#AA,#55,#FF");
    let _ = exec_line(&mut m, "?PEEK(#700)");
    let _ = exec_line(&mut m, "?PEEK(#701)");
    let t = screen(&m);
    assert!(t.contains("255"), "{t}");
    assert!(t.contains("170"), "{t}");
}
