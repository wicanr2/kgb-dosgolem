// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 無頭執行 KGB，接手文字 routine，輸出文字事件與 3 倍中文覆繪截圖。
//!
//! kgb-run --root <原版唯讀> --scratch <暫存層> --out <尚不存在的目錄>
//!         [--steps N] [--shots N] [--cycles N] [--tsv <譯文表>]... [--font <ttc> --font-index N]
//!         [--input <輸入腳本>] [--trace-phrases]
//!
//! 輸入腳本每行一個動作，依步數遞增，`#` 開頭為註解：
//!   <步數> mouse <x> <y> <按鍵>     x、y 為原版畫面座標（320×200），按鍵 bit0 左、bit1 右
//!   <步數> key <掃描碼> <ASCII> <0|1>   數值十六進位，1＝按下
//! `--trace-phrases` 另輸出 phrases.tsv：片語解碼器 53d8 的每次呼叫（取證用）。
use ferrogolem_core::machine::DEFAULT_IPS;
use ferrogolem_core::png::encode_rgb;
use kgb_cht::session::{Session, load_text};
use serde_json::json;
use std::{fs, path::PathBuf, process::ExitCode};

struct Args {
    root: PathBuf,
    scratch: PathBuf,
    out: PathBuf,
    steps: u64,
    shots: u64,
    cycles: u64,
    tsv: Vec<PathBuf>,
    font: Option<PathBuf>,
    font_index: u32,
    input: Option<PathBuf>,
    trace_phrases: bool,
}

enum Action {
    Mouse(u16, u16, u16),
    Key(u8, u8, bool),
}

fn parse_input(text: &str) -> Result<Vec<(u64, Action)>, String> {
    let mut out: Vec<(u64, Action)> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.is_empty() || f[0].starts_with('#') {
            continue;
        }
        let bad = || format!("輸入腳本第 {} 行格式錯誤：{line}", n + 1);
        let num = |s: &str| s.parse::<u64>().map_err(|_| bad());
        let hex = |s: &str| u8::from_str_radix(s, 16).map_err(|_| bad());
        let step = num(f[0])?;
        let act = match (f.get(1).copied(), f.len()) {
            (Some("mouse"), 5) => Action::Mouse(num(f[2])? as u16, num(f[3])? as u16, num(f[4])? as u16),
            (Some("key"), 5) => Action::Key(hex(f[2])?, hex(f[3])?, f[4] == "1"),
            _ => return Err(bad()),
        };
        if out.last().is_some_and(|(s, _)| *s > step) {
            return Err(format!("輸入腳本第 {} 行步數倒退", n + 1));
        }
        out.push((step, act));
    }
    Ok(out)
}

fn parse() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let (mut root, mut scratch, mut out) = (None, None, None);
    let (mut steps, mut shots, mut cycles) = (2_000_000, 0, DEFAULT_IPS / 1000);
    let (mut tsv, mut font, mut font_index) = (Vec::new(), None, 3);
    let (mut input, mut trace_phrases) = (None, false);
    while let Some(k) = it.next() {
        let mut v = || it.next().ok_or(format!("{k} 需要值"));
        match k.as_str() {
            "--root" => root = Some(PathBuf::from(v()?)),
            "--scratch" => scratch = Some(PathBuf::from(v()?)),
            "--out" => out = Some(PathBuf::from(v()?)),
            "--steps" => steps = v()?.parse().map_err(|e| format!("--steps：{e}"))?,
            "--shots" => shots = v()?.parse().map_err(|e| format!("--shots：{e}"))?,
            "--cycles" => cycles = v()?.parse().map_err(|e| format!("--cycles：{e}"))?,
            "--tsv" => tsv.push(PathBuf::from(v()?)),
            "--font" => font = Some(PathBuf::from(v()?)),
            "--font-index" => font_index = v()?.parse().map_err(|e| format!("--font-index：{e}"))?,
            "--input" => input = Some(PathBuf::from(v()?)),
            "--trace-phrases" => trace_phrases = true,
            _ => return Err(format!("未知參數 {k}")),
        }
    }
    Ok(Args {
        root: root.ok_or("缺 --root")?,
        scratch: scratch.ok_or("缺 --scratch")?,
        out: out.ok_or("缺 --out")?,
        steps,
        shots,
        cycles,
        tsv,
        font,
        font_index,
        input,
        trace_phrases,
    })
}

fn run(a: Args) -> Result<bool, String> {
    fs::create_dir(&a.out).map_err(|e| format!("建立 {}：{e}", a.out.display()))?;
    let (catalog, mut glyphs) = load_text(&a.tsv, a.font.as_ref(), a.font_index)?;
    let mut s = Session::new(&a.root, &a.scratch, a.cycles * 1000, catalog)?;
    let inputs = match &a.input {
        Some(p) => parse_input(&fs::read_to_string(p).map_err(|e| format!("{}：{e}", p.display()))?)?,
        None => Vec::new(),
    };
    if a.trace_phrases {
        s.phrase_log = Some(String::from("step\tret\tphrase\tphz\tblock\tindex\twidth\n"));
    }
    let mut error = None;
    let mut next = if a.shots > 0 { a.shots } else { a.steps };
    let mut pending = inputs.iter().peekable();
    while s.m.cpu.steps < a.steps && !s.m.exited && error.is_none() {
        let target = pending.peek().map_or(next, |(st, _)| (*st).min(next)).min(a.steps);
        if let Err(e) = s.run_until(target) {
            error = Some(e);
        }
        while let Some((_, act)) = pending.next_if(|(st, _)| *st <= s.m.cpu.steps) {
            match *act {
                Action::Mouse(x, y, b) => {
                    let (w, h, _) = s.m.bus.rgb();
                    s.m.mouse.move_to(f32::from(x) / w as f32, f32::from(y) / h as f32);
                    s.m.mouse.set_buttons(b);
                }
                Action::Key(scan, ascii, down) => s.m.key(scan, ascii, down),
            }
        }
        if a.shots > 0 && s.m.cpu.steps >= next {
            let (w, h, rgb) = s.compose(glyphs.as_mut());
            let name = format!("shot-{:010}.png", s.m.cpu.steps);
            fs::write(a.out.join(name), encode_rgb(w, h, &rgb)).map_err(|e| e.to_string())?;
            next += a.shots.max(1);
        } else if a.shots == 0 && s.m.cpu.steps >= next {
            next = a.steps;
        }
    }
    fs::write(a.out.join("events.tsv"), &s.log).map_err(|e| e.to_string())?;
    if let Some(log) = &s.phrase_log {
        fs::write(a.out.join("phrases.tsv"), log).map_err(|e| e.to_string())?;
    }
    let registers = {
        use ferrogolem_core::cpu::{AX, BP, BX, CS, CX, DI, DS, DX, ES, SI, SP, SS};
        let (r, g) = (&s.m.cpu.r, &s.m.cpu.s);
        format!(
            "AX={:04x} BX={:04x} CX={:04x} DX={:04x} SI={:04x} DI={:04x} BP={:04x} SP={:04x} DS={:04x} ES={:04x} SS={:04x} CS={:04x} IP={:04x}",
            r[AX], r[BX], r[CX], r[DX], r[SI], r[DI], r[BP], r[SP], g[DS], g[ES], g[SS], g[CS], s.m.cpu.ip
        )
    };
    let report = json!({
        "executed": s.m.cpu.steps, "error": error, "at": format!("{:04x}:{:04x}", s.m.cpu.start.0, s.m.cpu.start.1),
        "cycles_per_ms": a.cycles, "virtual_ms": u128::from(s.m.cpu.steps) * 1000 / u128::from(s.m.ips.max(1)),
        "events": s.events, "translated": s.translated, "catalog": s.catalog.len(),
        "overlay_removed": s.screen.removed,
        "signature_mismatches": s.take.mismatches.iter().map(|(o, n)| format!("{o:04x}×{n}")).collect::<Vec<_>>(),
        // 停點診斷：最近的讀檔、開檔、暫存器與音效狀態（與 fg-probe 同格式）。
        "registers": registers,
        "reads_dos": s.m.read_log.iter().rev().take(60).rev().map(|(st, h, a, want, got)| format!("#{st} h={h} -> {a:05x} want={want} got={got}")).collect::<Vec<_>>(),
        "files": s.m.file_log.iter().rev().take(40).rev().map(|e| format!("#{} {:02x} {} -> {}", e.step, e.ah, e.name, e.result)).collect::<Vec<_>>(),
        "sb": {"dma_request_count": s.m.bus.sb.dma_request_count, "dma_requests": format!("{:?}", s.m.bus.sb.dma_requests), "rate": s.m.bus.sb.rate, "speaker_on": s.m.bus.sb.speaker_on},
        "irq7_delivered": s.m.irq7_delivered,
    });
    fs::write(a.out.join("report.json"), serde_json::to_string_pretty(&report).unwrap()).map_err(|e| e.to_string())?;
    println!("executed={} events={} translated={} error={}", s.m.cpu.steps, s.events, s.translated, report["error"]);
    Ok(error.is_none())
}

fn main() -> ExitCode {
    match parse().and_then(run) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(3),
        Err(e) => {
            eprintln!("kgb-run: {e}");
            ExitCode::from(2)
        }
    }
}
