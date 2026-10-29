// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 可丟棄的研究探針：在 dosgolem-core 上執行一支 DOS 程式，跑到步數上限、
//! 程式結束或第一個不支援的指令／服務為止，寫出狀態 JSON 與畫面 PNG。
//! 不認識任何特定遊戲。
//!
//! dgrs-probe --root <DOS 根目錄> --exe <程式> [--tail <命令列尾>]
//!            [--scratch <可寫暫存層>] [--steps N] --out <尚不存在的輸出目錄>
use dosgolem_core::{
    cpu::{AX, BP, BX, CS, CX, DI, DS, DX, ES, SI, SP, SS},
    machine::Machine,
};
use serde_json::json;
use std::{fs, path::PathBuf, process::ExitCode};

struct Args {
    root: PathBuf,
    exe: String,
    tail: String,
    scratch: Option<PathBuf>,
    steps: u64,
    out: PathBuf,
}

fn parse() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mut scratch = None;
    let (mut root, mut exe, mut tail, mut steps, mut out) = (None, None, String::new(), 2_000_000, None);
    while let Some(k) = it.next() {
        let mut v = || it.next().ok_or(format!("{k} 需要值"));
        match k.as_str() {
            "--root" => root = Some(PathBuf::from(v()?)),
            "--exe" => exe = Some(v()?),
            "--tail" => tail = v()?,
            "--scratch" => scratch = Some(PathBuf::from(v()?)),
            "--steps" => steps = v()?.parse().map_err(|e| format!("--steps：{e}"))?,
            "--out" => out = Some(PathBuf::from(v()?)),
            _ => return Err(format!("未知參數 {k}")),
        }
    }
    Ok(Args {
        root: root.ok_or("缺 --root")?,
        exe: exe.ok_or("缺 --exe")?,
        tail,
        scratch,
        steps,
        out: out.ok_or("缺 --out")?,
    })
}

/// 未壓縮（stored deflate）的 RGB PNG，只用標準庫。
fn png(w: usize, h: usize, rgb: &[u8]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = !0u32;
        for &b in data {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc(&body).to_be_bytes());
    }
    let mut raw = Vec::with_capacity(h * (w * 3 + 1));
    for y in 0..h {
        raw.push(0);
        raw.extend(&rgb[y * w * 3..(y + 1) * w * 3]);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        z.extend((b.len() as u16).to_le_bytes());
        z.extend((!(b.len() as u16)).to_le_bytes());
        z.extend(*b);
    }
    let (mut a, mut s) = (1u32, 0u32);
    for &b in &raw {
        a = (a + u32::from(b)) % 65521;
        s = (s + a) % 65521;
    }
    z.extend(((s << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = (w as u32).to_be_bytes().to_vec();
    ihdr.extend((h as u32).to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn run(a: Args) -> Result<bool, String> {
    fs::create_dir(&a.out).map_err(|e| format!("建立 {}：{e}", a.out.display()))?;
    let mut m = match &a.scratch {
        Some(s) => Machine::with_scratch(&a.root, s)?,
        None => Machine::new(&a.root)?,
    };
    m.load_with_tail(&a.exe, a.tail.as_bytes())?;
    let mut error = None;
    while m.cpu.steps < a.steps && !m.exited {
        if let Err(e) = m.step() {
            error = Some(e);
            break;
        }
    }
    let (cs, ip) = m.cpu.start;
    let at = usize::from(cs) * 16 + usize::from(ip);
    let bytes: Vec<String> = (0..8)
        .map(|i| format!("{:02x}", m.bus.ram[(at + i) & 0xfffff]))
        .collect();
    let r = &m.cpu.r;
    let s = &m.cpu.s;
    let report = json!({
        "probe": "dgrs-probe",
        "exe": a.exe, "tail": a.tail, "step_limit": a.steps,
        "executed": m.cpu.steps, "exited": m.exited,
        "error": error,
        "at": format!("{cs:04x}:{ip:04x}"), "bytes": bytes.join(" "),
        "registers": format!(
            "AX={:04x} BX={:04x} CX={:04x} DX={:04x} SI={:04x} DI={:04x} BP={:04x} SP={:04x} DS={:04x} ES={:04x} SS={:04x} CS={:04x}",
            r[AX], r[BX], r[CX], r[DX], r[SI], r[DI], r[BP], r[SP], s[DS], s[ES], s[SS], s[CS]),
        "status": m.status(),
        "sb": {"dma_requests": format!("{:?}", m.bus.sb.dma_requests), "rate": m.bus.sb.rate, "time_constant": m.bus.sb.time_constant, "speaker_on": m.bus.sb.speaker_on},
        "dma": {"mask": m.bus.dma.mask, "mode": m.bus.dma.mode, "base": m.bus.dma.base, "page": format!("{:?}", m.bus.dma.page)},
        "files": m.file_log.iter().map(|e| format!("#{} {:02x} {} -> {}", e.step, e.ah, e.name, e.result)).collect::<Vec<_>>(),
        "irq7_delivered": m.irq7_delivered,
        "hooked_vectors": (0..256usize).filter(|n| m.bus.word(n * 4 + 2) != 0x80).map(|n| format!("{n:02x}={:04x}:{:04x}", m.bus.word(n * 4 + 2), m.bus.word(n * 4))).collect::<Vec<_>>(),
        "ems": {"handles": m.ems.layout(), "unsupported": m.ems.unsupported, "ops": m.ems.ops.iter().take(64).map(|o| json!({"step": o.step, "ah": o.ah, "al": o.al, "dx": o.handle, "bx": o.value, "status": o.status})).collect::<Vec<_>>()},
    });
    let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    fs::write(a.out.join("report.json"), &text).map_err(|e| e.to_string())?;
    let (w, h, rgb) = m.bus.rgb();
    fs::write(a.out.join("screen.png"), png(w, h, &rgb)).map_err(|e| e.to_string())?;
    println!(
        "executed={} exited={} at={cs:04x}:{ip:04x} bytes={} error={}",
        m.cpu.steps,
        m.exited,
        bytes.join(" "),
        report["error"]
    );
    Ok(report["error"].is_null())
}

fn main() -> ExitCode {
    match parse().and_then(run) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(3),
        Err(e) => {
            eprintln!("dgrs-probe: {e}");
            ExitCode::from(2)
        }
    }
}
