// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 一次 KGB 執行：機器、文字接手、覆繪生命週期與譯文表。無頭的 kgb-run 與視窗的
//! kgb-play 共用這份迴圈，行為只有一個來源。
use crate::catalog::{Catalog, key_of};
use crate::lifecycle::Screen;
use crate::overlay::{self, Glyphs, Item};
use crate::takeover::{BLOCK_DATA, Takeover, TextEvent, normalize};
use std::collections::HashMap;
use ferrogolem_core::cpu::{AX, CS, Cpu, DI, DS, ES, SP, SS};
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
    /// 片語解碼器 `53d8` 的呼叫紀錄（TSV，含表頭），只觀測、不改狀態；`None` 時不記。
    /// 取證多片語區塊用（worklist composite-blocks）。
    pub phrase_log: Option<String>,
    /// 各區塊（ES, DI）目前的片語起點，依 `53d8` 呼叫記錄（docs/spec/004 §7）。
    starts: HashMap<(u16, u16), Vec<u16>>,
}

/// 片語解碼器入口（docs/re/004）：AX＝片語偏移、DS＝PHZ 段、ES:DI＝文字區塊。
const PHRASE_DECODE: u16 = 0x53d8;

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
            log: String::from("step\tkind\tsite\tx\ty\tlines\tlh\tcolor\ttranslated\tkey\ttaken\tpage\n"),
            events: 0,
            translated: 0,
            phrase_log: None,
            starts: HashMap::new(),
        })
    }

    /// 執行到步數達 `until` 或程式結束。
    pub fn run_until(&mut self, until: u64) -> Result<(), String> {
        while self.m.cpu.steps < until && !self.m.exited {
            let c = &self.m.cpu;
            if c.ip == PHRASE_DECODE && c.s[CS] == self.m.psp.wrapping_add(0x10) {
                let (es, di) = (c.s[ES], c.r[DI]);
                let at = |o: u16| self.m.bus.word(Cpu::address(es, di.wrapping_add(o)));
                let start = at(2);
                let list = self.starts.entry((es, di)).or_default();
                if list.last().is_some_and(|&last| start <= last) {
                    list.clear();
                }
                list.push(start);
                if let Some(log) = &mut self.phrase_log {
                    let ret = self.m.bus.word(Cpu::address(c.s[SS], c.r[SP]));
                    let _ = writeln!(log, "{}\t{:04x}\t{:04x}\t{:04x}\t{:04x}\t{}\t{}", c.steps, ret, c.r[AX], c.s[DS], di, start, at(4));
                }
            }
            if let Some(mut e) = self.take.intercept(&mut self.m) {
                if e.kind == crate::takeover::Kind::Block {
                    e.segments = self.starts.get(&e.block).cloned().unwrap_or_default();
                }
                let (shown, found) = shown(&self.catalog, &e);
                self.events += 1;
                self.translated += u64::from(found);
                let _ = writeln!(
                    self.log,
                    "{}\t{:?}\t{:04x}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    e.step, e.kind, e.site, e.x, e.y, e.max_lines, e.line_height, e.color,
                    u8::from(found), key_of(&e.text), u8::from(e.taken), e.page
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

    /// 換譯文表：顯示中的項目改用新表的譯文。`passthrough` 為真時是英文原版，
    /// 之後不再跳過原版繪字，並清掉覆繪項目（被跳過的英文要等原版下次重畫才出現）。
    /// 回傳換下來的舊表。
    pub fn set_text(&mut self, catalog: Catalog, passthrough: bool) -> Catalog {
        let old = std::mem::replace(&mut self.catalog, catalog);
        self.take.passthrough = passthrough;
        if passthrough {
            self.screen.clear();
        } else {
            let c = &self.catalog;
            self.screen.retranslate(|item| shown(c, &item.event).0);
        }
        old
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

/// 覆繪要顯示的文字與是否找到譯文（docs/spec/004 §4 換頁、§7 多片語）：
/// 1. 本次原文查到譯文，顯示譯文第 0 頁；2. 頁序大於 0 時以整塊原文查，顯示第「頁序」頁；
/// 3. 有片語起點紀錄時逐段組合譯文，顯示第「頁序」頁；4. 都不行時以英文畫原版這次會畫的文字。
/// 譯文以 `^` 分頁。
fn shown(catalog: &Catalog, e: &TextEvent) -> (String, bool) {
    let page = |zh: &str, k: usize| {
        let pages: Vec<&str> = zh.split('^').collect();
        pages.get(k).or(pages.last()).map_or_else(String::new, |p| p.trim().to_string())
    };
    if let Some(zh) = catalog.get(&e.text) {
        return (page(zh, 0), true);
    }
    if e.page > 0 {
        if let Some(zh) = catalog.get(&e.full) {
            return (page(zh, usize::from(e.page)), true);
        }
    }
    if let Some((zh, all)) = compose(catalog, &e.raw, &e.segments) {
        if zh.chars().any(is_cjk) {
            return (page(&zh, usize::from(e.page)), all);
        }
    }
    (String::from_utf8_lossy(&e.drawn).into_owned(), false)
}

/// 片語段查譯文；查不到且結尾是遊戲另外附加的標點時，以去掉標點的字幹查，標點換成全形
/// （ex-office4：「KGB captain's uniform」之後由別的 routine 補「.」）。
fn lookup_with_punct(catalog: &Catalog, key: &[u8]) -> Option<String> {
    if let Some(zh) = catalog.get(key) {
        return Some(zh.to_string());
    }
    let (&last, stem) = key.split_last()?;
    let full = match last {
        b'.' => '。',
        b',' => '，',
        b'!' => '！',
        b'?' => '？',
        b':' => '：',
        b';' => '；',
        _ => return None,
    };
    let zh = catalog.get(normalize(stem.to_vec(), false).as_slice())?;
    Some(format!("{}{full}", zh.trim_end_matches(['。', '，', '！', '？', '：', '；'])))
}

fn is_cjk(c: char) -> bool {
    c >= '\u{2e80}'
}

/// 依片語起點把整塊 `[8, e)` 切段組合譯文。回傳組合結果與是否每個片語段都有譯文。
fn compose(catalog: &Catalog, raw: &[u8], starts: &[u16]) -> Option<(String, bool)> {
    let mut cuts: Vec<usize> = starts
        .iter()
        .filter_map(|&s| s.checked_sub(BLOCK_DATA).map(usize::from))
        .filter(|&c| c < raw.len())
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    if cuts.is_empty() {
        return None;
    }
    let mut pieces: Vec<(usize, usize, bool)> = Vec::new();
    if cuts[0] > 0 {
        pieces.push((0, cuts[0], false));
    }
    for (i, &c) in cuts.iter().enumerate() {
        pieces.push((c, cuts.get(i + 1).copied().unwrap_or(raw.len()), true));
    }
    let (mut out, mut all) = (String::new(), true);
    for (a, z, phrase) in pieces {
        let key = normalize(raw[a..z].to_vec(), false);
        if key.is_empty() {
            continue;
        }
        let part = match lookup_with_punct(catalog, &key) {
            Some(zh) if phrase => zh,
            _ => {
                all &= !phrase;
                String::from_utf8_lossy(&key).into_owned()
            }
        };
        let joined_cjk = out.chars().last().is_some_and(is_cjk) && part.chars().next().is_some_and(is_cjk);
        if !out.is_empty() && !joined_cjk {
            out.push(' ');
        }
        out.push_str(&part);
    }
    Some((out, all))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::hash_key;

    fn catalog(rows: &[(&[u8], &str)]) -> Catalog {
        let tsv: String = rows.iter().map(|(k, v)| format!("{}\t{v}\tt\n", hash_key(k))).collect();
        Catalog::parse(&tsv).unwrap()
    }

    #[test]
    fn composite_joins_phrase_translations() {
        // ex-office4：地點列 Inspect（起點 8）＋ yourself.（起點 16）。
        let c = catalog(&[(b"Inspect", "檢查"), (b"yourself.", "你自己。")]);
        let raw = b"Inspect yourself. ";
        assert_eq!(compose(&c, raw, &[8, 16]), Some(("檢查你自己。".into(), true)));
        assert_eq!(compose(&c, raw, &[]), None, "沒有片語紀錄就不組合");
    }

    #[test]
    fn appended_punctuation_falls_back_to_the_stem() {
        // ex-office4：「You're wearing」「a」「KGB captain's uniform」之後，遊戲另補「.」。
        let c = catalog(&[(b"You're wearing", "你穿著"), (b"a", "一件"), (b"KGB captain's uniform", "KGB 上尉制服")]);
        let raw = b"You're wearing a KGB captain's uniform.";
        assert_eq!(compose(&c, raw, &[8, 23, 25]), Some(("你穿著一件 KGB 上尉制服。".into(), true)));
    }

    #[test]
    fn gaps_stay_english_and_missing_phrases_are_reported() {
        let c = catalog(&[(b"hello.", "你好。")]);
        let raw = b"- Ivan: hello. bye.";
        // 片語起點：hello. 在索引 8＋8，bye. 在 8＋15；前面的「- Ivan:」不是片語。
        let (zh, all) = compose(&c, raw, &[16, 23]).unwrap();
        assert_eq!(zh, "- Ivan: 你好。 bye.");
        assert!(!all, "bye. 沒有譯文");
    }

    #[test]
    fn composite_pages_follow_carets() {
        let c = catalog(&[(b"One.^", "一。^"), (b"Two.", "二。")]);
        let mut e = TextEvent {
            step: 0,
            kind: crate::takeover::Kind::Block,
            site: 0x584f,
            text: b"Two.".to_vec(),
            x: 0,
            y: 0,
            max_lines: 5,
            width: 204,
            line_height: 12,
            color: 0,
            font_seg: 0,
            taken: true,
            page: 1,
            full: b"One.^Two.".to_vec(),
            drawn: b"Two.".to_vec(),
            block: (0x2144, 0x4434),
            raw: b"One.^Two.".to_vec(),
            segments: vec![8, 13],
        };
        assert_eq!(shown(&c, &e), ("二。".into(), true), "第二頁");
        e.page = 0;
        e.text = e.full.clone();
        assert_eq!(shown(&c, &e).0, "一。", "第一頁");
    }
}
