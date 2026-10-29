// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 以遊戲自己的片語解碼器萃取章節文字（docs/re/004-phrase-text.md）。
//!
//! 跑到章節已載入的狀態後，對 PHZ 片語表（段 `[2144:06DC]`）的每個項目呼叫
//! `188b:53d8`（AX＝片語偏移、DS＝PHZ 段、ES:DI＝文字區塊 2144:4434），區塊寬度設為
//! FFFFh 讓遊戲不插入排版斷行，執行到返回哨兵後讀出區塊內容。這是離線萃取用的拋棄式機器，
//! 會改寫區塊 4434 與解碼器的狀態變數，不用於遊玩。
//!
//! 插槽（docs/spec/004 §8）：解碼器遇到 1FFAh 以上的字碼時呼叫 `56d5`，以目前物件的名稱或
//! 預設代名詞填入。萃取時記下每次 `56d5` 寫入的範圍與字碼（AL），另輸出 `template` 欄：
//! 把插槽內容換成 `{字碼}`（兩位十六進位）。沒有插槽的片語該欄為空。
//!
//! kgb-extract --root <原版唯讀> --scratch <暫存層> --out <輸出 TSV> [--steps N] [--cycles N]
//!             [--sweep 起-迄|auto]   另從「起」依序解碼片語表之外的片語到「迄」（十六進位偏移）；
//!                                    auto 從表內最後一個片語的結尾掃到 PHZ 檔尾
//!             [--chapter N]          以遊戲的載檔程式 2394 換載第 N 章的 WRQ、PHZ 後再萃取（docs/re/004）
//! 輸出欄 bits_last：片語結束時最後載入的位元組被讀了幾個位元。
use ferrogolem_core::cpu::{AX, BX, CS, DI, DS, DX, ES, IF, SI, SP, SS};
use ferrogolem_core::cpu::Cpu;
use kgb_cht::catalog::key_of;
use kgb_cht::session::TAIL;
use ferrogolem_core::machine::Machine;
use std::{fmt::Write as _, fs, path::PathBuf, process::ExitCode};

const DATA: u16 = 0x2144 - 0x188b; // 主資料段相對 load segment 的位移
const DECODE: u16 = 0x53d8;
const BLOCK: u16 = 0x4434;
const SENTINEL: u16 = 0xfff0;
const SLOT: u16 = 0x56d5;

fn word(m: &Machine, seg: u16, off: u16) -> u16 {
    m.bus.word(Cpu::address(seg, off))
}

fn main_inner() -> Result<(), String> {
    let mut it = std::env::args().skip(1);
    let (mut root, mut scratch, mut out) = (None, None, None);
    let (mut steps, mut cycles) = (930_000_000u64, 1000u64);
    let mut sweep: Option<(u16, u16)> = None;
    let (mut sweep_auto, mut chapter) = (false, 1u16);
    while let Some(k) = it.next() {
        let mut v = || it.next().ok_or(format!("{k} 需要值"));
        match k.as_str() {
            "--root" => root = Some(PathBuf::from(v()?)),
            "--scratch" => scratch = Some(PathBuf::from(v()?)),
            "--out" => out = Some(PathBuf::from(v()?)),
            "--steps" => steps = v()?.parse().map_err(|e| format!("--steps：{e}"))?,
            "--cycles" => cycles = v()?.parse().map_err(|e| format!("--cycles：{e}"))?,
            "--chapter" => chapter = v()?.parse().map_err(|e| format!("--chapter：{e}"))?,
            "--sweep" => {
                let r = v()?;
                if r == "auto" {
                    sweep_auto = true;
                    continue;
                }
                let (a, z) = r.split_once('-').ok_or("--sweep 格式為 起-迄（十六進位）")?;
                let p = |x: &str| u16::from_str_radix(x, 16).map_err(|e| format!("--sweep：{e}"));
                sweep = Some((p(a)?, p(z)?));
            }
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
    let mut phz_len = None;
    if chapter != 1 {
        phz_len = Some(load_chapter(&mut m, code, data, chapter)?);
    }
    let phz = word(&m, data, 0x06dc);
    let count = word(&m, phz, 0) / 2;
    let mut tsv = String::from("index\toffset\tstatus\ttext\ttemplate\tsi_end\tbits_last\n");
    let (mut ok, mut failed, mut templates) = (0, 0, 0);
    let mut row = |label: String, off: u16, d: &Decoded| {
        let _ = writeln!(tsv, "{label}\t{off:04x}\t{}\t{}\t{}\t{:04x}\t{:04x}", d.status, key_of(&d.text), d.template, d.si_end, d.ax_end);
        if d.status == "ok" { ok += 1 } else { failed += 1 }
        templates += usize::from(!d.template.is_empty());
    };
    for i in 0..count {
        let off = word(&m, phz, i * 2);
        if off == 0 {
            continue;
        }
        let d = decode(&mut m, code, data, phz, off);
        row(i.to_string(), off, &d);
    }
    if sweep_auto {
        let last = word(&m, phz, (count - 1) * 2);
        let d = decode(&mut m, code, data, phz, last);
        let from = if d.ax_end <= 2 { d.si_end.wrapping_sub(1) } else { d.si_end };
        sweep = Some((from, phz_len.unwrap_or(0xe70a)));
    }
    // 片語表之外的片語（docs/re/004）：從「起」依序解碼。下一個片語的起點由最後載入的位元組
    // 讀了幾個位元決定：1–2 個時為返回 SI－1，3–8 個時為返回 SI（以片語表內 3,065 組相鄰
    // 片語歸納，沒有例外）。
    if let Some((from, to)) = sweep {
        let mut off = from;
        while off < to {
            let d = decode(&mut m, code, data, phz, off);
            row(format!("s{off:04x}"), off, &d);
            let next = if d.ax_end <= 2 { d.si_end.wrapping_sub(1) } else { d.si_end };
            if d.status != "ok" || next <= off {
                break;
            }
            off = next;
        }
    }
    fs::write(out.ok_or("缺 --out")?, tsv).map_err(|e| e.to_string())?;
    println!("phrases={count} ok={ok} failed={failed} templates={templates} phz={phz:04x}");
    Ok(())
}

/// 章節檔名表（LANG 資源段，第一章 5411h）：第一章的清單在 012Dh，第 2–4 章的清單指標在 0127h 起；
/// 每份清單依序為 WRQ、PHZ、SQX、OB、TH 的「旗標位元組＋檔名」位址，0 結束（docs/re/004）。
const NAMES: u16 = 0x5411 - 0x188b;
const FILE_LOAD: u16 = 0x2394;

/// 以遊戲的載檔程式 `2394`（ES:BX＝ES:DX＝旗標位元組（檔名在下一位元組）、DI:SI＝目的地）把第 N 章的 WRQ 與 PHZ
/// 載到第一章用的段（`[06da]`、`[06dc]`）。回傳 PHZ 的長度（位元組，最後一次讀檔實際讀到的數量）。
fn load_chapter(m: &mut Machine, code: u16, data: u16, chapter: u16) -> Result<u16, String> {
    let names = code + NAMES;
    let list = if chapter == 1 { 0x012d } else { word(m, names, 0x0127 + (chapter - 2) * 2) };
    let mut len = 0;
    for (k, target) in [(0u16, 0x06dau16), (1, 0x06dc)] {
        let entry = word(m, names, list + k * 2);
        let saved = m.cpu.clone();
        m.cpu.s[CS] = code;
        m.cpu.ip = FILE_LOAD;
        m.cpu.s[DS] = data;
        m.cpu.s[ES] = names;
        m.cpu.r[BX] = entry;
        m.cpu.r[DX] = entry; // 2394 自己把 DX 加 1 跳過旗標位元組（ex-open1：int 21h 時 DX＝BX＋1）
        m.cpu.r[DI] = word(m, data, target);
        m.cpu.r[SI] = 0;
        let (ss, sp) = (m.cpu.s[SS], m.cpu.r[SP].wrapping_sub(2));
        m.cpu.r[SP] = sp;
        m.bus.set_word(Cpu::address(ss, sp), SENTINEL);
        let limit = m.cpu.steps + 50_000_000;
        while !(m.cpu.s[CS] == code && m.cpu.ip == SENTINEL) {
            if m.cpu.steps > limit {
                return Err(format!("第 {chapter} 章載檔逾時"));
            }
            m.step()?;
        }
        // 2394 的成功路徑不清 CF（23ce–23d7 的移位留下的位元），以開檔紀錄判斷成敗。
        match m.file_log.last() {
            Some(e) if e.ah == 0x3d && e.result >= 0 => {}
            other => return Err(format!("第 {chapter} 章載檔失敗（{entry:04x}，{:?}）", other.map(|e| (&e.name, e.result)))),
        }
        // 精確長度取最後一次 DOS 讀檔實際讀到的位元組數（[0722] 是進位到段的長度，掃描會超出檔尾）。
        len = m.read_log.last().map_or(0, |r| r.4);
        m.cpu = saved;
    }
    Ok(len)
}

struct Decoded {
    status: &'static str,
    text: Vec<u8>,
    template: String,
    si_end: u16,
    ax_end: u16,
}

/// 以遊戲的解碼器解一個片語，記下插槽範圍，執行完還原 CPU。
fn decode(m: &mut Machine, code: u16, data: u16, phz: u16, off: u16) -> Decoded {
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
    // (起點索引, 字碼, 返回位址, 進入時 SP)；返回後補上終點。
    let mut open: Vec<(u16, u8, u16, u16)> = Vec::new();
    let mut slots: Vec<(u16, u16, u8)> = Vec::new();
    // 位元讀取器（5741、574d：shr al，空了才在 5745、5751 lods 下一個位元組）：
    // 記下最後載入的位元組被讀了幾個位元，用來判斷下一個片語的起點。
    let mut bits = 0u8;
    while !(m.cpu.s[CS] == code && m.cpu.ip == SENTINEL) {
        if m.cpu.s[CS] == code {
            match m.cpu.ip {
                0x5745 | 0x5751 => bits = 0,
                0x5746 | 0x5752 => bits = bits.saturating_add(1),
                0x5741 | 0x574d if m.cpu.r[AX] as u8 > 1 => bits = bits.saturating_add(1),
                _ => {}
            }
            let sp = m.cpu.r[SP];
            if m.cpu.ip == SLOT {
                let ret = m.bus.word(Cpu::address(m.cpu.s[SS], sp));
                open.push((m.bus.word(base + 2), m.cpu.r[AX] as u8, ret, sp));
            } else if let Some(&(start, code_, ret, at)) = open.last() {
                if m.cpu.ip == ret && sp == at.wrapping_add(2) {
                    open.pop();
                    slots.push((start, m.bus.word(base + 2), code_));
                }
            }
        }
        if m.cpu.steps > limit {
            status = "timeout";
            break;
        }
        if let Err(e) = m.step() {
            status = "error";
            eprintln!("片語 {off:04x}：{e}");
            break;
        }
    }
    let end = m.bus.word(base + 2);
    let at = |a: u16, z: u16| (a..z.max(a)).map(|k| m.bus.ram[base + usize::from(k)]).collect::<Vec<u8>>();
    let text = at(8, end);
    let mut template = String::new();
    if !slots.is_empty() {
        let mut pos = 8u16;
        slots.sort();
        for &(a, z, c) in &slots {
            let a = a.max(pos);
            template += &key_of(&at(pos, a));
            template += &format!("{{{c:02x}}}");
            pos = z.max(a);
        }
        template += &key_of(&at(pos, end));
    }
    let d = Decoded { status, text, template, si_end: m.cpu.r[SI], ax_end: u16::from(bits) };
    m.cpu = saved;
    d
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
