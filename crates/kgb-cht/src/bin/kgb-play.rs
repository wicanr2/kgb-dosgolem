// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 在 FerroGolem 視窗中玩 KGB：接手文字 routine，中文畫在 3 倍覆繪層。
//!
//! kgb-play --root <原版唯讀> --scratch <暫存層，需含 KGB\CSP.CFG>
//!          [--cycles 20000] [--tsv <譯文表>]... --font <ttc> [--font-index 3]
use ferrogolem_core::machine::DEFAULT_IPS;
use ferrogolem_core::mouse::x_scale;
use ferrogolem_frontend::{Guest, UiFont, run};
use kgb_cht::overlay::Glyphs;
use kgb_cht::session::{Session, load_text};
use std::{path::PathBuf, process::ExitCode};

struct Kgb {
    s: Session,
    glyphs: Option<Glyphs>,
}

impl Guest for Kgb {
    fn run_for(&mut self, steps: u64) -> Result<(), String> {
        let until = self.s.m.cpu.steps + steps;
        self.s.run_until(until)?;
        if self.s.m.exited { Err("遊戲已結束".into()) } else { Ok(()) }
    }
    fn frame(&mut self) -> (usize, usize, Vec<u8>) {
        self.s.compose(self.glyphs.as_mut())
    }
    fn key(&mut self, scan: u8, ascii: u8, down: bool) {
        self.s.m.key(scan, ascii, down)
    }
    fn pointer(&mut self, nx: f32, ny: f32, buttons: u16) {
        let (w, h, _) = self.s.m.bus.rgb();
        let m = &mut self.s.m;
        let xs = x_scale(m.bus.vga.mode);
        m.mouse.move_to((nx * w as f32) as u16, (ny * h as f32) as u16, xs);
        m.mouse.set_buttons(buttons);
    }
    fn ips(&self) -> u64 {
        self.s.m.ips
    }
    fn status(&self) -> String {
        format!(
            "KGB  虛擬 {:.1} 秒  文字 {}／譯 {}",
            self.s.m.cpu.steps as f64 / self.s.m.ips as f64,
            self.s.events,
            self.s.translated
        )
    }
}

fn main_inner() -> Result<(), String> {
    let mut it = std::env::args().skip(1);
    let (mut root, mut scratch, mut cycles) = (None, None, DEFAULT_IPS / 1000);
    let (mut tsv, mut font, mut font_index) = (Vec::new(), None, 3u32);
    while let Some(k) = it.next() {
        let mut v = || it.next().ok_or(format!("{k} 需要值"));
        match k.as_str() {
            "--root" => root = Some(PathBuf::from(v()?)),
            "--scratch" => scratch = Some(PathBuf::from(v()?)),
            "--cycles" => cycles = v()?.parse().map_err(|e| format!("--cycles：{e}"))?,
            "--tsv" => tsv.push(PathBuf::from(v()?)),
            "--font" => font = Some(PathBuf::from(v()?)),
            "--font-index" => font_index = v()?.parse().map_err(|e| format!("--font-index：{e}"))?,
            _ => return Err(format!("未知參數 {k}")),
        }
    }
    let font = font.ok_or("缺 --font")?;
    let (catalog, glyphs) = load_text(&tsv, Some(&font), font_index)?;
    let s = Session::new(&root.ok_or("缺 --root")?, &scratch.ok_or("缺 --scratch")?, cycles * 1000, catalog)?;
    let ui = UiFont { bytes: std::fs::read(&font).map_err(|e| e.to_string())?, index: font_index };
    run("KGB 繁體中文版（FerroGolem）", [960.0, 640.0], Some(ui), Box::new(Kgb { s, glyphs }))
}

fn main() -> ExitCode {
    match main_inner() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kgb-play: {e}");
            ExitCode::FAILURE
        }
    }
}
