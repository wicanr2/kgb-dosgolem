// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 以遊戲自己的圖塊程式把 SPRA.ANC 的每個圖塊畫成 PNG（取證用，docs/re/006）。
//!
//! 跑到章節已載入後，對圖塊庫 `[2144:06d2]` 的每個編號呼叫 `188b:85d5`（AX＝編號、CX＝x、BX＝y），
//! 圖塊畫進驅動的離屏緩衝（段＝驅動 `[020c]`），讀出非 0 像素的外框，以當時的 DAC 轉成 PNG。
//! 拋棄式機器：會清空離屏緩衝，不用於遊玩。
//!
//! kgb-sprites --root <原版唯讀> --scratch <暫存層> --out <輸出目錄> [--steps N] [--cycles N] [--bank 06d2]
use ferrogolem_core::cpu::{AX, BX, CS, CX, Cpu, DS, ES, IF, SP, SS};
use ferrogolem_core::machine::Machine;
use ferrogolem_core::png::encode_rgb;
use kgb_cht::session::TAIL;
use std::{fs, path::PathBuf, process::ExitCode};

const DATA: u16 = 0x2144 - 0x188b;
const DRIVER: u16 = 0x51e0 - 0x188b;
const SENTINEL: u16 = 0xfff0;

fn main_inner() -> Result<(), String> {
    let mut it = std::env::args().skip(1);
    let (mut root, mut scratch, mut out) = (None, None, None);
    let (mut steps, mut cycles, mut bank_var) = (1_000_000_000u64, 1000u64, 0x06d2u16);
    while let Some(k) = it.next() {
        let mut v = || it.next().ok_or(format!("{k} 需要值"));
        match k.as_str() {
            "--root" => root = Some(PathBuf::from(v()?)),
            "--scratch" => scratch = Some(PathBuf::from(v()?)),
            "--out" => out = Some(PathBuf::from(v()?)),
            "--steps" => steps = v()?.parse().map_err(|e| format!("--steps：{e}"))?,
            "--cycles" => cycles = v()?.parse().map_err(|e| format!("--cycles：{e}"))?,
            "--bank" => bank_var = u16::from_str_radix(&v()?, 16).map_err(|e| format!("--bank：{e}"))?,
            _ => return Err(format!("未知參數 {k}")),
        }
    }
    let out = out.ok_or("缺 --out")?;
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let mut m = Machine::with_scratch(&root.ok_or("缺 --root")?, &scratch.ok_or("缺 --scratch")?)?;
    m.ips = cycles * 1000;
    m.load_with_tail("CSPPRG.EXE", TAIL.as_bytes())?;
    while m.cpu.steps < steps {
        m.step()?;
    }
    let code = m.psp + 0x10;
    let data = code + DATA;
    let bank = m.bus.word(Cpu::address(data, bank_var));
    let buffer = m.bus.word(Cpu::address(code + DRIVER, 0x020c));
    let count = m.bus.word(Cpu::address(bank, 0)) / 2;
    let dac = m.bus.vga.dac.map(|c| c.map(|x| (x << 2) | (x >> 4)));
    let mut index = String::from("sprite\tx0\ty0\tw\th\n");
    for n in 1..=count {
        let saved = m.cpu.clone();
        let base = usize::from(buffer) << 4;
        m.bus.ram[base..base + 64000].fill(0);
        m.cpu.s[CS] = code;
        m.cpu.ip = 0x85d5;
        m.cpu.s[DS] = data;
        m.cpu.s[ES] = data;
        m.cpu.r[AX] = n;
        m.cpu.r[CX] = 0;
        m.cpu.r[BX] = 0;
        m.cpu.set_flag(IF, false);
        let (ss, sp) = (m.cpu.s[SS], m.cpu.r[SP].wrapping_sub(2));
        m.cpu.r[SP] = sp;
        m.bus.set_word(Cpu::address(ss, sp), SENTINEL);
        let limit = m.cpu.steps + 5_000_000;
        while !(m.cpu.s[CS] == code && m.cpu.ip == SENTINEL) && m.cpu.steps < limit {
            m.step()?;
        }
        let px = &m.bus.ram[base..base + 64000];
        let (mut x0, mut y0, mut x1, mut y1) = (320, 200, 0, 0);
        for (i, &p) in px.iter().enumerate() {
            if p != 0 {
                let (x, y) = (i % 320, i / 320);
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
        if x1 >= x0 && y1 >= y0 {
            let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
            let mut rgb = Vec::with_capacity(w * h * 3);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    rgb.extend(dac[usize::from(px[y * 320 + x])]);
                }
            }
            fs::write(out.join(format!("spra-{n:02}.png")), encode_rgb(w, h, &rgb)).map_err(|e| e.to_string())?;
            index += &format!("{n}\t{x0}\t{y0}\t{w}\t{h}\n");
        } else {
            index += &format!("{n}\t-\t-\t0\t0\n");
        }
        m.cpu = saved;
    }
    fs::write(out.join("index.tsv"), index).map_err(|e| e.to_string())?;
    println!("bank={bank:04x} buffer={buffer:04x} sprites={count}");
    Ok(())
}

fn main() -> ExitCode {
    match main_inner() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kgb-sprites: {e}");
            ExitCode::FAILURE
        }
    }
}
