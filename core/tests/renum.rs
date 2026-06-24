//! RENUM の再採番と GOTO/GOSUB 参照書換のテスト。

mod common;

use common::screen_text;
use ichigojam_core::{exec_line, Machine};

/// RENUM: 行番号を再採番する。GOTO/GOSUB を含まないプレーン行で行番号自体が
/// 振り直されること。参照書換は別テスト
/// (`renum_rewrites_goto_and_gosub_references`) で検証する。
#[test]
fn renum_renumbers_line_numbers_only() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "5 ?\"A\"");
    let _ = exec_line(&mut m, "7 ?\"B\"");
    let _ = exec_line(&mut m, "99 ?\"C\"");
    let _ = exec_line(&mut m, "RENUM 10,10");
    // 行番号が 10, 20, 30 に振り直される
    assert_eq!(m.list_get_number(0), 10);
    let mut idx = m.list_get_length(0) as u16 + 4;
    assert_eq!(m.list_get_number(idx), 20);
    idx += m.list_get_length(idx) as u16 + 4;
    assert_eq!(m.list_get_number(idx), 30);
}

/// RENUM が GOTO/GOSUB の数値リテラル参照も書き換える。
///
/// 桁数を変えない範囲 (2 桁→2 桁) で正しさを確認し、続けて 3 桁→1 桁
/// (縮小方向は本移植でも常に成功) のケースも検証する。
#[test]
fn renum_rewrites_goto_and_gosub_references() {
    // === 2 桁→2 桁 ===
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 GOTO 30");
    let _ = exec_line(&mut m, "20 ?\"MID\"");
    let _ = exec_line(&mut m, "30 GOSUB 20");
    let _ = exec_line(&mut m, "40 GOTO 10");
    let _ = exec_line(&mut m, "RENUM 50,10");
    // 行番号: 10,20,30,40 → 50,60,70,80
    assert_eq!(m.list_get_number(0), 50);
    // 参照: GOTO 30 → GOTO 70、GOSUB 20 → GOSUB 60、GOTO 10 → GOTO 50
    // (走らせて MID が出ることを 1 サイクル分で確認する。RUN 後に
    // GOSUB→RETURN→次行 GOTO 50 で 50 行目に戻ってループするため、
    // basic_step を有限回ぶん回して MID が出るかを見る。)
    let _ = exec_line(&mut m, "RUN");
    for _ in 0..500 {
        if m.basic_step().is_some() {
            break;
        }
        if screen_text(&m).contains("MID") {
            break;
        }
    }
    assert!(
        screen_text(&m).contains("MID"),
        "GOSUB 60 が 60 行目に届いていない: {:?}",
        screen_text(&m)
    );

    // === 3 桁→1 桁 (縮小) ===
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "100 GOTO 300");
    let _ = exec_line(&mut m, "200 ?\"OK\"");
    let _ = exec_line(&mut m, "300 GOSUB 200");
    let _ = exec_line(&mut m, "RENUM 1,1");
    assert_eq!(m.list_get_number(0), 1);
    let _ = exec_line(&mut m, "RUN");
    for _ in 0..200 {
        if m.basic_step().is_some() {
            break;
        }
        if screen_text(&m).contains("OK") {
            break;
        }
    }
    assert!(
        screen_text(&m).contains("OK"),
        "GOSUB 200→2 への縮小書換が機能していない: {:?}",
        screen_text(&m)
    );
}

/// RENUM が桁数オーバーで Illegal argument を返す: 1 桁参照 (例: `GOTO 5`) を
/// 3 桁の新番号に振り直すと行内バッファに収まらないため拒否する
/// (桁を詰め直すシフト処理は本移植では未対応)。
#[test]
fn renum_rejects_digit_overflow() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "5 GOTO 5");
    let res = exec_line(&mut m, "RENUM 100,100");
    assert!(res.is_err(), "1 桁→3 桁の参照書換は拒否されるべき");
}

/// RENUM の引数バリデーション: start<=0 または step<=0 は Illegal argument。
#[test]
fn renum_rejects_non_positive_args() {
    let mut m = Machine::new();
    let _ = exec_line(&mut m, "10 ?\"X\"");
    let res = exec_line(&mut m, "RENUM 0,10");
    assert!(res.is_err());
    let res = exec_line(&mut m, "RENUM 10,0");
    assert!(res.is_err());
}
