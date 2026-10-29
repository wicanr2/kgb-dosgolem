// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 一次 KGB 執行：機器、文字接手、覆繪生命週期與譯文表。無頭的 kgb-run 與視窗的
//! kgb-play 共用這份迴圈，行為只有一個來源。
use crate::catalog::{Catalog, key_of};
use crate::lifecycle::Screen;
use crate::overlay::{self, Glyphs, Item};
use crate::takeover::Takeover;
use ferrogolem_core::machine::Machine;
use std::fmt::Write as _;
use std::path::Path;

/// CONSP.BAT 的命令列尾。
pub const TAIL: &str = " AMR ADP220 SBP2227 EMS 386 SAF WRIC:\\KGB\\";

pub struct Session {
    pub m: Machine,
    pub take: Takeover,
    pub screen: Screen,
    pub catalog: Catalog,
    /// 文字事件紀錄（TSV，含表頭）。
    pub log: String,
    pub events: u64,
    pub translated: u64,
}

impl Session {
    /// root 為原版唯讀目錄，scratch 為可寫暫存層（需已含 `KGB\CSP.CFG`）。
    pub fn new(root: &Path, scratch: &Path, ips: u64, catalog: Catalog) -> Result<Self, String> {
        let mut m = Machine::with_scratch(root, scratch)?;
        m.ips = ips;
        m.load_with_tail("CSPPRG.EXE", TAIL.as_bytes())?;
        // 生命週期依 mode 13h 畫面寫入判斷（docs/spec/004 §5）。
        m.bus.write_watch = Some((0xa0000, 0xaf9ff));
        Ok(Self {
            m,
            take: Takeover::default(),
            screen: Screen::default(),
            catalog,
            log: String::from("step\tkind\tsite\tx\ty\tlines\tlh\tcolor\ttranslated\tkey\ttaken\n"),
            events: 0,
            translated: 0,
        })
    }

    /// 執行到步數達 `until` 或程式結束。
    pub fn run_until(&mut self, until: u64) -> Result<(), String> {
        while self.m.cpu.steps < until && !self.m.exited {
            if let Some(e) = self.take.intercept(&mut self.m) {
                let zh = self.catalog.get(&e.text);
                self.events += 1;
                self.translated += u64::from(zh.is_some());
                let _ = writeln!(
                    self.log,
                    "{}\t{:?}\t{:04x}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    e.step, e.kind, e.site, e.x, e.y, e.max_lines, e.line_height, e.color,
                    u8::from(zh.is_some()), key_of(&e.text), u8::from(e.taken)
                );
                // 未譯時以英文畫在覆繪層；原版 86e9 只畫前 max_lines 行，退路照做。
                let shown = zh.map_or_else(
                    || {
                        let s = String::from_utf8_lossy(&e.text);
                        s.split('^').take(usize::from(e.max_lines.max(1))).collect::<Vec<_>>().join("\n")
                    },
                    str::to_string,
                );
                let taken = e.taken;
                if taken && !e.text.is_empty() {
                    self.screen.on_event(Item { event: e, zh: shown });
                }
                if taken {
                    continue;
                }
            }
            self.m.step()?;
            if !self.m.bus.write_hits.is_empty() {
                self.screen.on_writes(self.m.cpu.steps, &self.m.bus.write_hits);
                self.m.bus.write_hits.clear();
            }
        }
        Ok(())
    }

    /// 原版 320×200 時合成 3 倍中文覆繪，其他模式原樣回傳。
    pub fn compose(&self, glyphs: Option<&mut Glyphs>) -> (usize, usize, Vec<u8>) {
        let (w, h, rgb) = self.m.bus.rgb();
        match (glyphs, w, h) {
            (Some(g), 320, 200) => {
                let dac = self.m.bus.vga.dac.map(|c| c.map(|x| (x << 2) | (x >> 4)));
                (overlay::W, overlay::H, overlay::compose(&rgb, &self.screen.items(), &dac, g))
            }
            _ => (w, h, rgb),
        }
    }
}

/// 讀譯文表與字型，並檢查譯文用到的每個字字型都有。
pub fn load_text(tsv: &[std::path::PathBuf], font: Option<&std::path::PathBuf>, index: u32) -> Result<(Catalog, Option<Glyphs>), String> {
    let mut text = String::new();
    for p in tsv {
        text += &std::fs::read_to_string(p).map_err(|e| format!("{}：{e}", p.display()))?;
        text.push('\n');
    }
    let catalog = if text.is_empty() { Catalog::default() } else { Catalog::parse(&text)? };
    let glyphs = match font {
        Some(p) => Some(Glyphs::load(std::fs::read(p).map_err(|e| e.to_string())?, index)?),
        None => None,
    };
    if let Some(g) = &glyphs {
        let missing: String = catalog.chars().into_iter().filter(|&c| c != '\n' && !g.has(c)).collect();
        if !missing.is_empty() {
            return Err(format!("字型缺字：{missing}"));
        }
    }
    Ok((catalog, glyphs))
}
