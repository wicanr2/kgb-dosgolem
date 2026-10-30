// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 在 FerroGolem 視窗中玩 KGB：接手文字 routine，中文畫在 3 倍覆繪層。
//! 工具列可切換繁中、簡中、英文原版；預設依系統語系（kgb_cht::lang）。
//!
//! kgb-play --root <原版唯讀> --scratch <暫存層，需含 KGB\CSP.CFG>
//!          --text <譯文目錄> --font <Noto Sans CJK ttc> [--lang zh-TW|zh-CN|en] [--cycles 20000]
//! 不帶 --root 時是發行包模式：從執行檔旁找 text/、font/、original/，存檔放使用者資料目錄。
//!
//! 譯文目錄下的 `*.zh-TW.tsv` 給繁中、`*.zh-CN.tsv` 給簡中；字型取 TTC 的 TC（3）與 SC（2）。
//! 同目錄的 `sprite-labels.tsv` 是圖塊文字標籤表（docs/spec/005）。
use ferrogolem_core::machine::DEFAULT_IPS;
use ferrogolem_frontend::{Guest, UiFont, egui, run};
use kgb_cht::catalog::Catalog;
use kgb_cht::lang::Lang;
use kgb_cht::overlay::Glyphs;
use kgb_cht::session::{Session, load_text};
use std::{path::Path, path::PathBuf, process::ExitCode};

/// 一種中文的譯文表與字型。
struct Text {
    lang: Lang,
    catalog: Option<Catalog>,
    glyphs: Glyphs,
}

struct Kgb {
    s: Session,
    lang: Lang,
    /// 各中文版本；目前語言的譯文表在 Session 裡，這裡留空。
    texts: Vec<Text>,
}

impl Kgb {
    fn switch(&mut self, to: Lang) {
        if to == self.lang {
            return;
        }
        let incoming = match self.texts.iter_mut().find(|t| t.lang == to) {
            Some(t) => t.catalog.take().unwrap_or_default(),
            None => Catalog::default(),
        };
        let old = self.s.set_text(incoming, to == Lang::En);
        self.s.sprites.simplified = to == Lang::ZhCn;
        if let Some(t) = self.texts.iter_mut().find(|t| t.lang == self.lang) {
            t.catalog = Some(old);
        }
        self.lang = to;
    }
}

impl Guest for Kgb {
    fn run_for(&mut self, steps: u64) -> Result<(), String> {
        let until = self.s.m.cpu.steps + steps;
        self.s.run_until(until)?;
        if self.s.m.exited { Err("遊戲已結束".into()) } else { Ok(()) }
    }
    fn frame(&mut self) -> (usize, usize, Vec<u8>) {
        let lang = self.lang;
        let glyphs = self.texts.iter_mut().find(|t| t.lang == lang).map(|t| &mut t.glyphs);
        self.s.compose(glyphs)
    }
    fn key(&mut self, scan: u8, ascii: u8, down: bool) {
        self.s.m.key(scan, ascii, down)
    }
    fn pointer(&mut self, nx: f32, ny: f32, buttons: u16) {
        self.s.m.mouse.move_to(nx, ny);
        self.s.m.mouse.set_buttons(buttons);
    }
    fn ips(&self) -> u64 {
        self.s.m.ips
    }
    fn status(&self) -> String {
        format!(
            "虛擬 {:.1} 秒  文字 {}／譯 {}",
            self.s.m.cpu.steps as f64 / self.s.m.ips as f64,
            self.s.events,
            self.s.translated
        )
    }
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        for l in Lang::ALL {
            if ui.selectable_label(self.lang == l, l.label()).clicked() {
                self.switch(l);
            }
        }
        ui.separator();
    }
}

/// 譯文目錄下某語言的全部譯文表，依檔名排序。
fn catalogs_for(dir: &Path, tag: &str) -> Result<Vec<PathBuf>, String> {
    let suffix = format!(".{tag}.tsv");
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}：{e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&suffix)))
        .collect();
    v.sort();
    Ok(v)
}

fn main_inner() -> Result<(), String> {
    let mut it = std::env::args().skip(1);
    let (mut root, mut scratch, mut cycles) = (None, None, DEFAULT_IPS / 1000);
    let (mut text, mut font, mut lang) = (None, None, None);
    while let Some(k) = it.next() {
        let mut v = || it.next().ok_or(format!("{k} 需要值"));
        match k.as_str() {
            "--root" => root = Some(PathBuf::from(v()?)),
            "--scratch" => scratch = Some(PathBuf::from(v()?)),
            "--cycles" => cycles = v()?.parse().map_err(|e| format!("--cycles：{e}"))?,
            "--text" => text = Some(PathBuf::from(v()?)),
            "--font" => font = Some(PathBuf::from(v()?)),
            "--lang" => {
                let s = v()?;
                lang = Some(Lang::parse(&s).ok_or(format!("--lang 只接受 zh-TW、zh-CN、en，收到 {s}"))?);
            }
            _ => return Err(format!("未知參數 {k}")),
        }
    }
    // 發行包：不帶 --root 時從執行檔位置找包內的譯文、字型與原版，存檔放使用者資料目錄（kgb_cht::install）。
    if root.is_none() {
        use kgb_cht::install;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let base = install::find_base(exe.parent().ok_or("執行檔沒有上層目錄")?).ok_or("找不到包內的 text/ 目錄")?;
        let l = install::layout(&base);
        let data = install::data_dir(std::env::consts::OS, |k| std::env::var(k).ok()).ok_or("找不到使用者資料目錄")?;
        let (cd, save) = install::prepare(&l, &data)?;
        root = Some(cd);
        scratch = scratch.or(Some(save));
        text = text.or(Some(l.text));
        font = font.or(Some(l.font));
    }
    let (text, font) = (text.ok_or("缺 --text")?, font.ok_or("缺 --font")?);
    let lang = lang.unwrap_or_else(|| Lang::from_env(|k| std::env::var(k).ok()));
    let mut texts = Vec::new();
    for l in Lang::ALL {
        let (Some(tag), Some(index)) = (l.tag(), l.font_index()) else { continue };
        let (catalog, glyphs) = load_text(&catalogs_for(&text, tag)?, Some(&font), index)?;
        texts.push(Text { lang: l, catalog: Some(catalog), glyphs: glyphs.ok_or("字型未載入")? });
    }
    let start = texts.iter_mut().find(|t| t.lang == lang).and_then(|t| t.catalog.take()).unwrap_or_default();
    let mut s = Session::new(&root.ok_or("缺 --root")?, &scratch.ok_or("缺 --scratch")?, cycles * 1000, start)?;
    s.take.passthrough = lang == Lang::En;
    // 圖塊文字標籤表（docs/spec/005）；沒有這個檔就不畫圖塊譯文。
    if let Ok(tsv) = std::fs::read_to_string(text.join("sprite-labels.tsv")) {
        s.sprites = kgb_cht::sprites::Sprites::new(kgb_cht::sprites::parse_labels(&tsv)?);
        s.sprites.simplified = lang == Lang::ZhCn;
    }
    if let Ok(tsv) = std::fs::read_to_string(text.join("card-labels.tsv")) {
        s.sprites.set_cards(kgb_cht::sprites::parse_cards(&tsv)?);
    }
    // 介面字用 TC；Noto Sans CJK 每個 collection 都含繁簡字形，「简中」照樣顯示。
    let ui = UiFont { bytes: std::fs::read(&font).map_err(|e| e.to_string())?, index: 3 };
    run("KGB 陰謀（FerroGolem）", [960.0, 660.0], Some(ui), Box::new(Kgb { s, lang, texts }))
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
