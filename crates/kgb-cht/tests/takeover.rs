// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 文字 routine 接手（docs/spec/004 §3、§6）：以最小 MZ 程式在真實偏移放簽章，
//! 從遊戲相同的呼叫點呼叫，驗證跳過後的暫存器與簽章不符時不攔截。
use ferrogolem_core::cpu::{AX, BP, BX, CS, CX, DI, DX, SI, SP};
use ferrogolem_core::machine::Machine;
use kgb_cht::takeover::{BLOCK, Kind, LINE, Takeover};
use std::fs;

const BLOCK_SIG: [u8; 16] = [
    0x50, 0x52, 0x56, 0x55, 0x06, 0x53, 0x51, 0x8d, 0x31, 0x8b, 0x4d, 0x02, 0x2b, 0xcb, 0xbb, 0x00,
];
const LINE_SIG: [u8; 16] = [
    0xfc, 0x55, 0x56, 0x06, 0x8a, 0x36, 0x0e, 0x44, 0x8b, 0xf7, 0x8b, 0xf9, 0x8b, 0xcb, 0xac, 0x3c,
];

/// 影像 0x8800 bytes，填 HLT。在 `at` 放 mov 序列與 `call target`，入口為序列開頭。
fn build(entry: u16, movs: &[(u8, u16)], call_at: u16, target: u16, corrupt: bool) -> Vec<u8> {
    let mut img = vec![0xf4u8; 0x8800];
    img[usize::from(BLOCK)..usize::from(BLOCK) + 16].copy_from_slice(&BLOCK_SIG);
    img[usize::from(LINE)..usize::from(LINE) + 16].copy_from_slice(&LINE_SIG);
    if corrupt {
        img[usize::from(target) + 5] ^= 0xff;
    }
    let mut p = usize::from(entry);
    for &(op, v) in movs {
        img[p] = op;
        img[p + 1..p + 3].copy_from_slice(&v.to_le_bytes());
        p += 3;
    }
    assert_eq!(p, usize::from(call_at));
    img[p] = 0xe8;
    let rel = target.wrapping_sub(call_at + 3);
    img[p + 1..p + 3].copy_from_slice(&rel.to_le_bytes());
    let size = 32 + img.len();
    let mut h = vec![0u8; 32];
    let w = |h: &mut Vec<u8>, at: usize, v: u16| h[at..at + 2].copy_from_slice(&v.to_le_bytes());
    h[0..2].copy_from_slice(b"MZ");
    w(&mut h, 2, (size % 512) as u16);
    w(&mut h, 4, size.div_ceil(512) as u16);
    w(&mut h, 8, 2);
    w(&mut h, 0x0a, 0x100);
    w(&mut h, 0x0c, 0xffff);
    w(&mut h, 0x0e, 0x0900);
    w(&mut h, 0x10, 0x0200);
    w(&mut h, 0x14, entry);
    w(&mut h, 0x18, 0x1c);
    h.extend(img);
    h
}

fn run_to(name: &str, exe: Vec<u8>, target: u16, setup: impl Fn(&mut Machine)) -> (Machine, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("kgb-takeover-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("T.EXE"), exe).unwrap();
    let mut m = Machine::new(&dir).unwrap();
    m.load("T.EXE").unwrap();
    setup(&mut m);
    while m.cpu.ip != target {
        m.step().unwrap();
        assert!(m.cpu.steps < 100, "沒有走到 {target:04x}");
    }
    (m, dir)
}

fn block_setup(m: &mut Machine) {
    let base = usize::from(m.psp) * 16 + 0x100;
    m.bus.set_word(base + 2, 8 + 11);
    m.bus.set_word(base + 4, 320);
    m.bus.ram[base + 8..base + 8 + 11].copy_from_slice(b"Hello^world");
}

#[test]
fn layout_breaks_become_spaces_in_key() {
    // 5bed 自動換行把空白改成 0；比對鍵要還原成空白，結尾的 0 與空白去掉。
    let movs = [(0xbf, 0x100), (0xbb, 8), (0xb9, 0), (0xba, 0xb5), (0xbd, 2)];
    let (mut m, dir) = run_to("wrap", build(0x2cd6, &movs, 0x2ce5, BLOCK, false), BLOCK, |m| {
        let base = usize::from(m.psp) * 16 + 0x100;
        m.bus.set_word(base + 2, 8 + 14);
        m.bus.ram[base + 8..base + 8 + 14].copy_from_slice(b"series of\0incs");
        m.bus.ram[base + 8 + 13] = b' ';
    });
    let e = Takeover::default().intercept(&mut m).unwrap();
    assert_eq!(e.text, b"series of inc");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn block_is_skipped_and_bx_advances_past_the_page() {
    let movs = [(0xbf, 0x100), (0xbb, 8), (0xb9, 0), (0xba, 0xb5), (0xbd, 2)];
    let (mut m, dir) = run_to("block", build(0x2cd6, &movs, 0x2ce5, BLOCK, false), BLOCK, block_setup);
    let before = m.cpu.r;
    let mut t = Takeover::default();
    let e = t.intercept(&mut m).expect("應攔截");
    assert_eq!((e.kind, e.site, e.x, e.y, e.max_lines, e.width), (Kind::Block, 0x2ce8, 0, 181, 2, 320));
    assert_eq!(e.text, b"Hello^world", "原文的 ^ 保留");
    assert!(e.taken);
    assert_eq!((e.page, e.drawn.as_slice()), (0, &b"Hello"[..]), "原版這次只畫第一頁");
    assert_eq!(m.cpu.ip, 0x2ce8, "模擬近返回");
    assert_eq!(m.cpu.r[BX], 8 + 6, "BX 越過第一頁的 ^（docs/spec/004 §3）");
    for r in [AX, CX, DX, SI, DI, BP] {
        assert_eq!(m.cpu.r[r], before[r], "暫存器 {r} 不變");
    }
    assert_eq!(m.cpu.r[SP], before[SP].wrapping_add(2));
    assert_eq!(m.cpu.s[CS], m.psp + 0x10);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn line_outputs_match_routine_contract() {
    let movs = [(0xbf, 0x200), (0xb9, 10), (0xbb, 50), (0xba, 0x1234)];
    let (mut m, dir) = run_to("line", build(0x1d88, &movs, 0x1d94, LINE, false), LINE, |m| {
        let base = usize::from(m.psp) * 16;
        m.bus.ram[base + 0x200..base + 0x206].copy_from_slice(b"HELLO^");
        m.bus.ram[base + 0x440e] = 0x77;
    });
    let mut t = Takeover::default();
    let e = t.intercept(&mut m).expect("應攔截");
    assert_eq!((e.kind, e.site, e.x, e.y, e.text.as_slice()), (Kind::Line, 0x1d97, 10, 50, &b"HELLO"[..]));
    assert_eq!(m.cpu.r[DI], 0x206, "DI 指向結尾字元之後");
    assert_eq!(m.cpu.r[CX], 50, "CX＝輸入 BX");
    assert_eq!(m.cpu.r[DX], 0x7734, "DH＝[440e]，DL 不變");
    assert_eq!(m.cpu.r[AX] & 0xff, u16::from(b'^'));
    assert_eq!(m.cpu.ip, 0x1d97);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn signature_mismatch_is_not_intercepted() {
    // 負對照：簽章被改一個位元組就不攔截，並記錄一次。
    let movs = [(0xbf, 0x100), (0xbb, 8), (0xb9, 0), (0xba, 0xb5), (0xbd, 2)];
    let (mut m, dir) = run_to("bad", build(0x2cd6, &movs, 0x2ce5, BLOCK, true), BLOCK, block_setup);
    let mut t = Takeover::default();
    assert!(t.intercept(&mut m).is_none());
    assert_eq!(t.mismatches, vec![(BLOCK, 1)]);
    assert_eq!(m.cpu.ip, BLOCK);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn untaken_site_is_observed_only() {
    // 不在接手清單的呼叫點（例如時鐘）只記錄，CPU 不變。
    let movs = [(0xbf, 0x200), (0xb9, 10), (0xbb, 50), (0xba, 0x1234)];
    let (mut m, dir) = run_to("observe", build(0x1000, &movs, 0x100c, LINE, false), LINE, |m| {
        let base = usize::from(m.psp) * 16;
        m.bus.ram[base + 0x200..base + 0x203].copy_from_slice(b"4/0");
    });
    let before = (m.cpu.r, m.cpu.ip);
    let mut t = Takeover::default();
    let e = t.intercept(&mut m).expect("仍記錄事件");
    assert!(!e.taken);
    assert_eq!((m.cpu.r, m.cpu.ip), before);
    fs::remove_dir_all(dir).unwrap();
}
