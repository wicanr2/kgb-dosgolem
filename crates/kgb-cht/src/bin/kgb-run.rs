// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 無頭執行 KGB，接手文字 routine，輸出文字事件與 3 倍中文覆繪截圖。
//!
//! kgb-run --root <原版唯讀> --scratch <暫存層> --out <尚不存在的目錄>
//!         [--steps N] [--shots N] [--cycles N] [--tsv <譯文表>]... [--font <ttc> --font-index N]
//!         [--input <輸入腳本>] [--trace-phrases] [--trace-writers <起始步數>] [--watch lo-hi] [--trace-driver <起始步數>] [--trace-at 偏移,...] [--labels <圖塊標籤表>] [--simplified]
//!
//! 輸入腳本每行一個動作，依步數遞增，`#` 開頭為註解：
//!   <步數> mouse <x> <y> <按鍵>     x、y 為原版畫面座標（320×200），按鍵 bit0 左、bit1 右
//!   <步數> key <掃描碼> <ASCII> <0|1>   數值十六進位，1＝按下
//! `--trace-phrases` 另輸出 phrases.tsv：片語解碼器 53d8 的每次呼叫（取證用）。
//! `--trace-writers N` 另輸出 writers.tsv：第 N 步起依指令 CS:IP 彙整的畫面寫入（取證用）。
//! `--trace-at 85f3,53d8` 另輸出 trace-at.tsv：進入這些 load segment 內偏移時的呼叫者與暫存器。
//! `--trace-driver N` 另輸出 driver.tsv：第 N 步起每次進入驅動跳躍表的入口、呼叫者與暫存器。
//! `--watch lo-hi` 把寫入監看改到另一個線性範圍（十六進位，例：離屏緩衝 28d10-3854f）；
//! 覆繪生命週期依 A0000 判斷，改了之後該次執行的覆繪不可信，只用於取證。
//! `--chapter N` 取證用：第一次進入開章程式 `1bb9` 時以第 N 章開始（只改 AL，docs/re/006）。
//! `--shot-at 步數,...` 只在這些步數存截圖（可與 `--shots` 並用）。
//! `--end-credits 步數` 取證用：過了這個步數後從腳本的片尾路徑播片尾名單（只改 IP，docs/spec/006 §4）。
//! `--dump-ram 步數,...` 取證用：在這些步數把 1 MiB 記憶體寫成 ram-<步數>.bin（每檔 1 MiB，張數自己控制）。
use ferrogolem_core::machine::DEFAULT_IPS;
use ferrogolem_core::png::encode_rgb;
use kgb_cht::session::{Session, load_text};
use serde_json::json;
use std::fmt::Write as _;
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
    trace_writers: Option<u64>,
    watch: Option<(usize, usize)>,
    trace_driver: Option<u64>,
    trace_at: Vec<u16>,
    labels: Option<PathBuf>,
    simplified: bool,
    chapter: Option<u8>,
    dump_ram: Vec<u64>,
    shot_at: Vec<u64>,
    end_credits: Option<u64>,
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
    let (mut input, mut trace_phrases, mut trace_writers, mut watch, mut trace_driver) = (None, false, None, None, None);
    let mut trace_at = Vec::new();
    let (mut labels, mut simplified, mut chapter, mut dump_ram) = (None, false, None, Vec::new());
    let (mut shot_at, mut end_credits) = (Vec::new(), None);
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
            "--watch" => {
                let r = v()?;
                let (lo, hi) = r.split_once('-').ok_or("--watch 格式為 lo-hi")?;
                let h = |x: &str| usize::from_str_radix(x, 16).map_err(|e| format!("--watch：{e}"));
                watch = Some((h(lo)?, h(hi)?));
            }
            "--labels" => labels = Some(PathBuf::from(v()?)),
            "--simplified" => simplified = true,
            "--chapter" => {
                let n: u8 = v()?.parse().map_err(|e| format!("--chapter：{e}"))?;
                if !(1..=4).contains(&n) {
                    return Err(format!("--chapter 只接受 1–4，收到 {n}"));
                }
                chapter = Some(n);
            }
            "--dump-ram" => {
                for x in v()?.split(',') {
                    dump_ram.push(x.parse().map_err(|e| format!("--dump-ram：{e}"))?);
                }
                dump_ram.sort_unstable();
            }
            "--end-credits" => end_credits = Some(v()?.parse().map_err(|e| format!("--end-credits：{e}"))?),
            "--shot-at" => {
                for x in v()?.split(',') {
                    shot_at.push(x.parse().map_err(|e| format!("--shot-at：{e}"))?);
                }
                shot_at.sort_unstable();
            }
            "--trace-at" => {
                for x in v()?.split(',') {
                    trace_at.push(u16::from_str_radix(x, 16).map_err(|e| format!("--trace-at：{e}"))?);
                }
            }
            "--trace-driver" => trace_driver = Some(v()?.parse().map_err(|e| format!("--trace-driver：{e}"))?),
            "--trace-writers" => trace_writers = Some(v()?.parse().map_err(|e| format!("--trace-writers：{e}"))?),
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
        trace_writers,
        watch,
        trace_driver,
        trace_at,
        labels,
        simplified,
        chapter,
        dump_ram,
        shot_at,
        end_credits,
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
    if let Some(p) = &a.labels {
        let tsv = fs::read_to_string(p).map_err(|e| format!("{}：{e}", p.display()))?;
        s.sprites = kgb_cht::sprites::Sprites::new(kgb_cht::sprites::parse_labels(&tsv)?);
        s.sprites.simplified = a.simplified;
        if let Ok(cards) = fs::read_to_string(p.with_file_name("card-labels.tsv")) {
            s.sprites.set_cards(kgb_cht::sprites::parse_cards(&cards)?);
        }
    }
    s.start_chapter = a.chapter;
    s.end_credits = a.end_credits;
    if !a.trace_at.is_empty() {
        s.trace_at = Some((a.trace_at.clone(), String::from("step\tat\tret\tregs\n")));
    }
    if let Some(from) = a.trace_driver {
        s.driver_log = Some((from, String::from("step\tentry\tcaller\tregs\n")));
    }
    if a.watch.is_some() {
        s.m.bus.write_watch = a.watch;
    }
    if let Some(from) = a.trace_writers {
        s.writers = Some((from, Default::default()));
    }
    if a.trace_phrases {
        s.phrase_log = Some(String::from("step\tret\tphrase\tphz\tblock\tindex\twidth\n"));
    }
    let mut error = None;
    let mut next = if a.shots > 0 { a.shots } else { a.steps };
    let mut pending = inputs.iter().peekable();
    let mut dumps = a.dump_ram.iter().peekable();
    let mut shots_at = a.shot_at.iter().peekable();
    while s.m.cpu.steps < a.steps && !s.m.exited && error.is_none() {
        let target = pending.peek().map_or(next, |(st, _)| (*st).min(next)).min(a.steps);
        let target = dumps.peek().map_or(target, |&&d| d.min(target));
        let target = shots_at.peek().map_or(target, |&&d| d.min(target));
        if let Err(e) = s.run_until(target) {
            error = Some(e);
        }
        while let Some(&d) = dumps.next_if(|&&d| d <= s.m.cpu.steps) {
            fs::write(a.out.join(format!("ram-{d:010}.bin")), &s.m.bus.ram).map_err(|e| e.to_string())?;
        }
        if shots_at.next_if(|&&d| d <= s.m.cpu.steps).is_some() {
            while shots_at.next_if(|&&d| d <= s.m.cpu.steps).is_some() {}
            let (w, h, rgb) = s.compose(glyphs.as_mut());
            fs::write(a.out.join(format!("shot-{:010}.png", s.m.cpu.steps)), encode_rgb(w, h, &rgb)).map_err(|e| e.to_string())?;
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
    if let Some((_, log)) = &s.trace_at {
        fs::write(a.out.join("trace-at.tsv"), log).map_err(|e| e.to_string())?;
    }
    if let Some((_, log)) = &s.driver_log {
        fs::write(a.out.join("driver.tsv"), log).map_err(|e| e.to_string())?;
    }
    if let Some((_, map)) = &s.writers {
        let mut t = String::from("cs:ip\tn\tfirst\tlo\thi\n");
        for ((cs, ip), (n, first, lo, hi)) in map {
            let _ = writeln!(t, "{cs:04x}:{ip:04x}\t{n}\t{first}\t{lo:05x}\t{hi:05x}");
        }
        fs::write(a.out.join("writers.tsv"), t).map_err(|e| e.to_string())?;
    }
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
