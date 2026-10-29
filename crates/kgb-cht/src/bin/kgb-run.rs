// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 無頭執行 KGB，接手文字 routine，輸出文字事件與 3 倍中文覆繪截圖。
//!
//! kgb-run --root <原版唯讀> --scratch <暫存層> --out <尚不存在的目錄>
//!         [--steps N] [--shots N] [--tsv <譯文表>]... [--font <ttc> --font-index N]
use ferrogolem_core::machine::Machine;
use ferrogolem_core::png::encode_rgb;
use kgb_cht::catalog::{Catalog, key_of};
use kgb_cht::lifecycle::Screen;
use kgb_cht::overlay::{self, Glyphs, Item};
use kgb_cht::takeover::Takeover;
use serde_json::json;
use std::{fmt::Write as _, fs, path::PathBuf, process::ExitCode};

const TAIL: &str = " AMR ADP220 SBP2227 EMS 386 SAF WRIC:\\KGB\\";

struct Args {
    root: PathBuf,
    scratch: PathBuf,
    out: PathBuf,
    steps: u64,
    shots: u64,
    tsv: Vec<PathBuf>,
    font: Option<PathBuf>,
    font_index: u32,
}

fn parse() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let (mut root, mut scratch, mut out) = (None, None, None);
    let (mut steps, mut shots, mut tsv, mut font, mut font_index) = (2_000_000, 0, Vec::new(), None, 3);
    while let Some(k) = it.next() {
        let mut v = || it.next().ok_or(format!("{k} 需要值"));
        match k.as_str() {
            "--root" => root = Some(PathBuf::from(v()?)),
            "--scratch" => scratch = Some(PathBuf::from(v()?)),
            "--out" => out = Some(PathBuf::from(v()?)),
            "--steps" => steps = v()?.parse().map_err(|e| format!("--steps：{e}"))?,
            "--shots" => shots = v()?.parse().map_err(|e| format!("--shots：{e}"))?,
            "--tsv" => tsv.push(PathBuf::from(v()?)),
            "--font" => font = Some(PathBuf::from(v()?)),
            "--font-index" => font_index = v()?.parse().map_err(|e| format!("--font-index：{e}"))?,
            _ => return Err(format!("未知參數 {k}")),
        }
    }
    Ok(Args {
        root: root.ok_or("缺 --root")?,
        scratch: scratch.ok_or("缺 --scratch")?,
        out: out.ok_or("缺 --out")?,
        steps,
        shots,
        tsv,
        font,
        font_index,
    })
}

fn dac8(m: &Machine) -> [[u8; 3]; 256] {
    m.bus.vga.dac.map(|c| c.map(|x| (x << 2) | (x >> 4)))
}

fn run(a: Args) -> Result<bool, String> {
    fs::create_dir(&a.out).map_err(|e| format!("建立 {}：{e}", a.out.display()))?;
    let mut catalog = Catalog::default();
    let mut text = String::new();
    for p in &a.tsv {
        text += &fs::read_to_string(p).map_err(|e| format!("{}：{e}", p.display()))?;
        text.push('\n');
    }
    if !text.is_empty() {
        catalog = Catalog::parse(&text)?;
    }
    let mut glyphs = match &a.font {
        Some(p) => Some(Glyphs::load(fs::read(p).map_err(|e| e.to_string())?, a.font_index)?),
        None => None,
    };
    if let Some(g) = &glyphs {
        let missing: String = catalog.chars().into_iter().filter(|&c| c != '\n' && !g.has(c)).collect();
        if !missing.is_empty() {
            return Err(format!("字型缺字：{missing}"));
        }
    }
    let mut m = Machine::with_scratch(&a.root, &a.scratch)?;
    m.load_with_tail("CSPPRG.EXE", TAIL.as_bytes())?;
    let mut take = Takeover::default();
    let mut screen = Screen::default();
    // 生命週期依 mode 13h 畫面寫入判斷（docs/spec/004 §5）。
    m.bus.write_watch = Some((0xa0000, 0xaf9ff));
    let mut log = String::from("step\tkind\tsite\tx\ty\tlines\tlh\tcolor\ttranslated\tkey\n");
    let (mut events, mut translated) = (0u64, 0u64);
    let mut error = None;
    let mut next_shot = a.shots;
    while m.cpu.steps < a.steps && !m.exited {
        if let Some(e) = take.intercept(&mut m) {
            let zh = catalog.get(&e.text);
            events += 1;
            translated += u64::from(zh.is_some());
            let _ = writeln!(
                log,
                "{}\t{:?}\t{:04x}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                e.step, e.kind, e.site, e.x, e.y, e.max_lines, e.line_height, e.color,
                u8::from(zh.is_some()), key_of(&e.text)
            );
            // 未譯的句子以英文原文畫在覆繪層，不讓文字消失。
            let shown = zh.map_or_else(|| String::from_utf8_lossy(&e.text).replace('^', "\n"), str::to_string);
            if !e.text.is_empty() {
                screen.on_event(Item { event: e, zh: shown });
            }
            continue;
        }
        if let Err(e) = m.step() {
            error = Some(e);
            break;
        }
        if !m.bus.write_hits.is_empty() {
            screen.on_writes(m.cpu.steps, &m.bus.write_hits);
            m.bus.write_hits.clear();
        }
        if a.shots > 0 && m.cpu.steps >= next_shot {
            next_shot += a.shots;
            let (w, h, rgb) = m.bus.rgb();
            let name = format!("shot-{:010}.png", m.cpu.steps);
            if let (Some(g), 320, 200) = (glyphs.as_mut(), w, h) {
                let big = overlay::compose(&rgb, &screen.items(), &dac8(&m), g);
                fs::write(a.out.join(name), encode_rgb(overlay::W, overlay::H, &big)).map_err(|e| e.to_string())?;
            } else {
                fs::write(a.out.join(name), encode_rgb(w, h, &rgb)).map_err(|e| e.to_string())?;
            }
        }
    }
    fs::write(a.out.join("events.tsv"), &log).map_err(|e| e.to_string())?;
    let report = json!({
        "executed": m.cpu.steps, "error": error, "at": format!("{:04x}:{:04x}", m.cpu.start.0, m.cpu.start.1),
        "events": events, "translated": translated, "catalog": catalog.len(),
        "overlay_removed": screen.removed,
        "dac_of_items": screen.items().iter().map(|it| format!("{}={:?}", it.event.color, m.bus.vga.dac[usize::from(it.event.color)])).collect::<Vec<_>>(),
        "signature_mismatches": take.mismatches.iter().map(|(o, n)| format!("{o:04x}×{n}")).collect::<Vec<_>>(),
    });
    fs::write(a.out.join("report.json"), serde_json::to_string_pretty(&report).unwrap()).map_err(|e| e.to_string())?;
    println!("executed={} events={events} translated={translated} error={}", m.cpu.steps, report["error"]);
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
