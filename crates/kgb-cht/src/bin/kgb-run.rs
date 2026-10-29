// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 無頭執行 KGB，接手文字 routine，輸出文字事件與 3 倍中文覆繪截圖。
//!
//! kgb-run --root <原版唯讀> --scratch <暫存層> --out <尚不存在的目錄>
//!         [--steps N] [--shots N] [--cycles N] [--tsv <譯文表>]... [--font <ttc> --font-index N]
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
}

fn parse() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let (mut root, mut scratch, mut out) = (None, None, None);
    let (mut steps, mut shots, mut cycles) = (2_000_000, 0, DEFAULT_IPS / 1000);
    let (mut tsv, mut font, mut font_index) = (Vec::new(), None, 3);
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
    })
}

fn run(a: Args) -> Result<bool, String> {
    fs::create_dir(&a.out).map_err(|e| format!("建立 {}：{e}", a.out.display()))?;
    let (catalog, mut glyphs) = load_text(&a.tsv, a.font.as_ref(), a.font_index)?;
    let mut s = Session::new(&a.root, &a.scratch, a.cycles * 1000, catalog)?;
    let mut error = None;
    let mut next = if a.shots > 0 { a.shots } else { a.steps };
    while s.m.cpu.steps < a.steps && !s.m.exited && error.is_none() {
        if let Err(e) = s.run_until(next.min(a.steps)) {
            error = Some(e);
        }
        if a.shots > 0 && s.m.cpu.steps >= next {
            let (w, h, rgb) = s.compose(glyphs.as_mut());
            let name = format!("shot-{:010}.png", s.m.cpu.steps);
            fs::write(a.out.join(name), encode_rgb(w, h, &rgb)).map_err(|e| e.to_string())?;
        }
        next += a.shots.max(1);
    }
    fs::write(a.out.join("events.tsv"), &s.log).map_err(|e| e.to_string())?;
    let report = json!({
        "executed": s.m.cpu.steps, "error": error, "at": format!("{:04x}:{:04x}", s.m.cpu.start.0, s.m.cpu.start.1),
        "cycles_per_ms": a.cycles, "virtual_ms": u128::from(s.m.cpu.steps) * 1000 / u128::from(s.m.ips.max(1)),
        "events": s.events, "translated": s.translated, "catalog": s.catalog.len(),
        "overlay_removed": s.screen.removed,
        "signature_mismatches": s.take.mismatches.iter().map(|(o, n)| format!("{o:04x}×{n}")).collect::<Vec<_>>(),
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
