// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 以遊戲自己的片語解碼器萃取章節文字（docs/re/004-phrase-text.md）。
//!
//! 跑到章節已載入的狀態後，對 PHZ 片語表（段 `[2144:06DC]`）的每個項目呼叫
//! `188b:53d8`（AX＝片語偏移、DS＝PHZ 段、ES:DI＝文字區塊 2144:4434），區塊寬度設為
//! FFFFh 讓遊戲不插入排版斷行，執行到返回哨兵後讀出區塊內容。這是離線萃取用的拋棄式機器，
//! 會改寫區塊 4434 與解碼器的狀態變數，不用於遊玩。
//!
//! kgb-extract --root <原版唯讀> --scratch <暫存層> --out <輸出 TSV> [--steps N] [--cycles N]
use ferrogolem_core::cpu::{AX, CS, DI, DS, ES, IF, SP, SS};
use ferrogolem_core::cpu::Cpu;
use kgb_cht::catalog::key_of;
use kgb_cht::session::TAIL;
use ferrogolem_core::machine::Machine;
use std::{fmt::Write as _, fs, path::PathBuf, process::ExitCode};

const DATA: u16 = 0x2144 - 0x188b; // 主資料段相對 load segment 的位移
const DECODE: u16 = 0x53d8;
const BLOCK: u16 = 0x4434;
const SENTINEL: u16 = 0xfff0;

fn word(m: &Machine, seg: u16, off: u16) -> u16 {
    m.bus.word(Cpu::address(seg, off))
}

fn main_inner() -> Result<(), String> {
    let mut it = std::env::args().skip(1);
    let (mut root, mut scratch, mut out) = (None, None, None);
    let (mut steps, mut cycles) = (930_000_000u64, 1000u64);
    while let Some(k) = it.next() {
        let mut v = || it.next().ok_or(format!("{k} 需要值"));
        match k.as_str() {
            "--root" => root = Some(PathBuf::from(v()?)),
            "--scratch" => scratch = Some(PathBuf::from(v()?)),
            "--out" => out = Some(PathBuf::from(v()?)),
            "--steps" => steps = v()?.parse().map_err(|e| format!("--steps：{e}"))?,
            "--cycles" => cycles = v()?.parse().map_err(|e| format!("--cycles：{e}"))?,
            _ => return Err(format!("未知參數 {k}")),
        }
    }
    let mut m = Machine::with_scratch(&root.ok_or("缺 --root")?, &scratch.ok_or("缺 --scratch")?)?;
    m.ips = cycles * 1000;
    m.load_with_tail("CSPPRG.EXE", TAIL.as_bytes())?;
    while m.cpu.steps < steps {
        m.step()?;
    }
    let code = m.psp + 0x10;
    let data = code + DATA;
    let phz = word(&m, data, 0x06dc);
    let count = word(&m, phz, 0) / 2;
    let mut tsv = String::from("index\toffset\tstatus\ttext\n");
    let (mut ok, mut failed) = (0, 0);
    for i in 0..count {
        let off = word(&m, phz, i * 2);
        if off == 0 {
            continue;
        }
        let saved = m.cpu.clone();
        let base = Cpu::address(data, BLOCK);
        m.bus.set_word(base + 2, 8);
        m.bus.set_word(base + 4, 0xffff);
        m.bus.set_word(base + 6, 0);
        m.cpu.s[CS] = code;
        m.cpu.ip = DECODE;
        m.cpu.s[DS] = phz;
        m.cpu.s[ES] = data;
        m.cpu.r[DI] = BLOCK;
        m.cpu.r[AX] = off;
        m.cpu.set_flag(IF, false);
        let (ss, sp) = (m.cpu.s[SS], m.cpu.r[SP].wrapping_sub(2));
        m.cpu.r[SP] = sp;
        m.bus.set_word(Cpu::address(ss, sp), SENTINEL);
        let limit = m.cpu.steps + 2_000_000;
        let mut status = "ok";
        while !(m.cpu.s[CS] == code && m.cpu.ip == SENTINEL) {
            if m.cpu.steps > limit {
                status = "timeout";
                break;
            }
            if let Err(e) = m.step() {
                status = "error";
                eprintln!("片語 {i}：{e}");
                break;
            }
        }
        let end = m.bus.word(base + 2);
        let text: Vec<u8> = (8..end.max(8)).map(|k| m.bus.ram[base + usize::from(k)]).collect();
        let _ = writeln!(tsv, "{i}\t{off:04x}\t{status}\t{}", key_of(&text));
        if status == "ok" { ok += 1 } else { failed += 1 }
        m.cpu = saved;
    }
    fs::write(out.ok_or("缺 --out")?, tsv).map_err(|e| e.to_string())?;
    println!("phrases={count} ok={ok} failed={failed} phz={phz:04x}");
    Ok(())
}

fn main() -> ExitCode {
    match main_inner() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kgb-extract: {e}");
            ExitCode::FAILURE
        }
    }
}
