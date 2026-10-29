// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 一次 KGB 執行：機器、文字接手、覆繪生命週期與譯文表。無頭的 kgb-run 與視窗的
//! kgb-play 共用這份迴圈，行為只有一個來源。
use crate::catalog::{Catalog, key_of};
use crate::lifecycle::Screen;
use crate::overlay::{self, Glyphs, Item};
use crate::sprites::Sprites;
use crate::takeover::{BLOCK_DATA, Segment, Takeover, TextEvent, normalize};
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
    /// 圖塊文字覆繪（docs/spec/005）。
    pub sprites: Sprites,
    /// 取證用：從某步數起，依寫入指令的 CS:IP 彙整畫面寫入（次數、首次步數、位址範圍）。
    pub writers: Option<(u64, std::collections::BTreeMap<(u16, u16), (u64, u64, usize, usize)>)>,
    /// 取證用：進入這些 load segment 內偏移時記錄呼叫者（近返回位址）與暫存器。
    pub trace_at: Option<(Vec<u16>, String)>,
    /// 取證用：從某步數起記錄進入驅動程式跳躍表（驅動段 0100h–01FFh）的呼叫與暫存器。
    pub driver_log: Option<(u64, String)>,
    /// 進行中的 `53d8`、`56d5` 呼叫。
    frames: Vec<Frame>,
    /// 各區塊（ES, DI）已完成的片語與插槽範圍（docs/spec/004 §7、§8）。
    segments: HashMap<(u16, u16), Vec<Segment>>,
}

/// 片語解碼器入口（docs/re/004）：AX＝片語偏移、DS＝PHZ 段、ES:DI＝文字區塊。
const PHRASE_DECODE: u16 = 0x53d8;
/// 插槽填入（docs/spec/004 §8）：AL＝字碼，ES:DI＝文字區塊。
const SLOT_FILL: u16 = 0x56d5;

/// 一次進行中的呼叫：以返回位址與進入時的 SP 判斷何時返回。
struct Frame {
    block: (u16, u16),
    start: u16,
    ret: u16,
    sp: u16,
    slot: Option<u8>,
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
            log: String::from("step\tkind\tsite\tx\ty\tlines\tlh\tcolor\ttranslated\tkey\ttaken\tpage\tsegments\n"),
            events: 0,
            translated: 0,
            phrase_log: None,
            sprites: Sprites::default(),
            writers: None,
            driver_log: None,
            trace_at: None,
            frames: Vec::new(),
            segments: HashMap::new(),
        })
    }

    /// 執行到步數達 `until` 或程式結束。
    pub fn run_until(&mut self, until: u64) -> Result<(), String> {
        while self.m.cpu.steps < until && !self.m.exited {
            if self.m.cpu.s[CS] == self.m.psp.wrapping_add(0x10) {
                self.track_segments();
                if !self.take.passthrough {
                    self.sprites.observe(&self.m);
                }
                if let Some((at, log)) = &mut self.trace_at {
                    let c = &self.m.cpu;
                    if at.contains(&c.ip) {
                        use ferrogolem_core::cpu::{BP, BX, CX, DX, SI};
                        let ret = self.m.bus.word(Cpu::address(c.s[SS], c.r[SP]));
                        let _ = writeln!(
                            log,
                            "{}\t{:04x}\t{:04x}\tAX={:04x} BX={:04x} CX={:04x} DX={:04x} SI={:04x} DI={:04x} BP={:04x} DS={:04x} ES={:04x}",
                            c.steps, c.ip, ret, c.r[AX], c.r[BX], c.r[CX], c.r[DX], c.r[SI], c.r[DI], c.r[BP], c.s[DS], c.s[ES]
                        );
                    }
                }
            }
            if let Some((from, log)) = &mut self.driver_log {
                let c = &self.m.cpu;
                // 驅動段＝load segment＋3955h（188Bh→51E0h，docs/spec/004 §2 的字形驅動）。
                if c.steps >= *from && c.s[CS] == self.m.psp.wrapping_add(0x10 + 0x3955) && (0x100..0x200).contains(&c.ip) {
                    let st = |o: u16| self.m.bus.word(Cpu::address(c.s[SS], c.r[SP].wrapping_add(o)));
                    let _ = writeln!(
                        log,
                        "{}\t{:04x}\t{:04x}:{:04x}\tAX={:04x} BX={:04x} CX={:04x} DX={:04x} SI={:04x} DI={:04x} BP={:04x} DS={:04x} ES={:04x}",
                        c.steps, c.ip, st(2), st(0), c.r[AX], c.r[ferrogolem_core::cpu::BX], c.r[ferrogolem_core::cpu::CX], c.r[ferrogolem_core::cpu::DX],
                        c.r[ferrogolem_core::cpu::SI], c.r[DI], c.r[ferrogolem_core::cpu::BP], c.s[DS], c.s[ES]
                    );
                }
            }
            if let Some(mut e) = self.take.intercept(&mut self.m) {
                if e.kind == crate::takeover::Kind::Block {
                    e.segments = self.segments.get(&e.block).cloned().unwrap_or_default();
                }
                let (shown, found) = shown(&self.catalog, &e);
                self.events += 1;
                self.translated += u64::from(found);
                let _ = writeln!(
                    self.log,
                    "{}\t{:?}\t{:04x}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    e.step, e.kind, e.site, e.x, e.y, e.max_lines, e.line_height, e.color,
                    u8::from(found), key_of(&e.text), u8::from(e.taken), e.page,
                    e.segments
                        .iter()
                        .map(|g| format!("{}-{}:{}{}", g.start, g.end, g.depth, g.slot.map_or(String::new(), |c| format!(":{c:02x}"))))
                        .collect::<Vec<_>>()
                        .join(" ")
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
                if let Some((from, map)) = &mut self.writers {
                    if self.m.cpu.steps >= *from {
                        let e = map.entry(self.m.cpu.start).or_insert((0, self.m.cpu.steps, usize::MAX, 0));
                        for &a in &self.m.bus.write_hits {
                            e.0 += 1;
                            e.2 = e.2.min(a);
                            e.3 = e.3.max(a);
                        }
                    }
                }
                self.screen.on_writes(self.m.cpu.steps, &self.m.bus.write_hits);
                self.m.bus.write_hits.clear();
            }
        }
        Ok(())
    }

    /// 追蹤 `53d8` 與 `56d5` 的進出（只讀，不改狀態）。CS 已確認是 load segment。
    fn track_segments(&mut self) {
        let c = &self.m.cpu;
        let (ip, sp) = (c.ip, c.r[SP]);
        if ip == PHRASE_DECODE || ip == SLOT_FILL {
            let block = (c.s[ES], c.r[DI]);
            let start = self.m.bus.word(Cpu::address(block.0, block.1.wrapping_add(2)));
            let ret = self.m.bus.word(Cpu::address(c.s[SS], sp));
            let slot = (ip == SLOT_FILL).then_some(c.r[AX] as u8);
            if ip == PHRASE_DECODE {
                if let Some(log) = &mut self.phrase_log {
                    let width = self.m.bus.word(Cpu::address(block.0, block.1.wrapping_add(4)));
                    let _ = writeln!(log, "{}\t{:04x}\t{:04x}\t{:04x}\t{:04x}\t{}\t{}", c.steps, ret, c.r[AX], c.s[DS], block.1, start, width);
                }
                if self.frames.is_empty() {
                    let list = self.segments.entry(block).or_default();
                    let last = list.iter().filter(|g| g.depth == 0 && g.slot.is_none()).map(|g| g.start).max();
                    if last.is_some_and(|l| start <= l) {
                        list.clear();
                    }
                }
            }
            if self.frames.len() >= 32 {
                self.frames.clear();
            }
            self.frames.push(Frame { block, start, ret, sp, slot });
        } else if let Some(f) = self.frames.last() {
            if ip == f.ret && sp == f.sp.wrapping_add(2) {
                let f = self.frames.pop().unwrap();
                let end = self.m.bus.word(Cpu::address(f.block.0, f.block.1.wrapping_add(2)));
                let depth = self.frames.len() as u8;
                self.segments.entry(f.block).or_default().push(Segment { start: f.start, end, depth, slot: f.slot });
            }
        }
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
    pub fn compose(&mut self, glyphs: Option<&mut Glyphs>) -> (usize, usize, Vec<u8>) {
        let (w, h, rgb) = self.m.bus.rgb();
        match (glyphs, w, h) {
            (Some(g), 320, 200) => {
                let dac = self.m.bus.vga.dac.map(|c| c.map(|x| (x << 2) | (x >> 4)));
                let patches = if self.take.passthrough || self.m.bus.vga.mode != 0x13 {
                    Vec::new()
                } else {
                    let indexed = self.m.bus.vga.indexed13(&self.m.bus.ram[0xa0000..0xb0000]);
                    self.sprites.patches(&indexed, &dac, self.m.cpu.steps, self.m.ips)
                };
                (overlay::W, overlay::H, overlay::compose(&rgb, &self.screen.items(), &patches, &dac, g))
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
    // 對話選單行尾的 7Fh 是遊戲字型的右箭頭（子選單，docs/spec/004 §3）。
    if let Some(stem) = e.text.strip_suffix(&[0x7f]) {
        let mut inner = e.clone();
        inner.text = normalize(stem.to_vec(), false);
        inner.drawn = inner.text.clone();
        let (zh, found) = shown(catalog, &inner);
        return (format!("{zh} ▶"), found);
    }
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

/// 依片語與插槽範圍組合整塊 `[8, e)` 的譯文（docs/spec/004 §7、§8）。
/// 回傳組合結果與是否每個片語段都有譯文；沒有片語紀錄時回 None。
fn compose(catalog: &Catalog, raw: &[u8], segments: &[Segment]) -> Option<(String, bool)> {
    let end = BLOCK_DATA.saturating_add(raw.len() as u16);
    if !segments.iter().any(|g| g.depth == 0 && g.slot.is_none() && g.start >= BLOCK_DATA && g.start < end) {
        return None;
    }
    Some(compose_range(catalog, raw, segments, BLOCK_DATA, end, 0))
}

/// 區塊索引 i 的位元組（raw 從索引 8 起）。
fn bytes(raw: &[u8], a: u16, z: u16) -> &[u8] {
    let len = raw.len();
    let (a, z) = (usize::from(a.saturating_sub(BLOCK_DATA)).min(len), usize::from(z.saturating_sub(BLOCK_DATA)).min(len));
    &raw[a..z.max(a)]
}

/// 組合 `[lo, hi)`：取深度 `depth` 的片語段，其間的字（名稱、標點）保留原文。
fn compose_range(catalog: &Catalog, raw: &[u8], segments: &[Segment], lo: u16, hi: u16, depth: u8) -> (String, bool) {
    let mut tops: Vec<&Segment> = segments
        .iter()
        .filter(|g| g.depth == depth && g.slot.is_none() && g.start >= lo && g.start < hi)
        .collect();
    tops.sort_by_key(|g| g.start);
    let mut pieces: Vec<(String, bool)> = Vec::new();
    let mut pos = lo;
    for g in tops {
        let z = g.end.min(hi).max(g.start);
        if g.start > pos {
            pieces.push(gap(bytes(raw, pos, g.start)));
        }
        pieces.push(phrase(catalog, raw, segments, g.start, z, depth));
        pos = z;
    }
    if pos < hi {
        pieces.push(gap(bytes(raw, pos, hi)));
    }
    let (mut out, mut all) = (String::new(), true);
    for (part, found) in pieces {
        if part.is_empty() {
            continue;
        }
        all &= found;
        let joined_cjk = out.chars().last().is_some_and(is_cjk) && part.chars().next().is_some_and(is_cjk);
        if !out.is_empty() && !joined_cjk {
            out.push(' ');
        }
        out.push_str(&part);
    }
    (out, all)
}

/// 片語段之間的字保留原文；沒有字母時不算缺譯文。插槽名稱前的冠詞（`5718` 從字詞表附加，
/// 例：「the 」＋遞迴片語「portrait」）中文不用，略去（docs/spec/004 §8）。
fn gap(b: &[u8]) -> (String, bool) {
    let key = normalize(b.to_vec(), false);
    if ARTICLES.iter().any(|a| a.eq_ignore_ascii_case(&key)) {
        return (String::new(), true);
    }
    (String::from_utf8_lossy(&key).into_owned(), !key.iter().any(u8::is_ascii_alphabetic))
}

const ARTICLES: [&[u8]; 3] = [b"the", b"a", b"an"];

/// 一個片語段：沒有插槽時整段查譯文；有插槽時組出範本鍵、查範本譯文，再填入插槽譯文（§8）。
fn phrase(catalog: &Catalog, raw: &[u8], segments: &[Segment], a: u16, z: u16, depth: u8) -> (String, bool) {
    let mut slots: Vec<&Segment> = segments
        .iter()
        .filter(|g| g.depth == depth + 1 && g.slot.is_some() && g.start >= a && g.end <= z)
        .collect();
    slots.sort_by_key(|g| g.start);
    let plain = normalize(bytes(raw, a, z).to_vec(), false);
    if slots.is_empty() {
        return match lookup_with_punct(catalog, &plain) {
            Some(zh) => (zh, true),
            None => (String::from_utf8_lossy(&plain).into_owned(), false),
        };
    }
    let (mut key, mut fillers, mut pos) = (Vec::new(), Vec::new(), a);
    for g in slots {
        key.extend_from_slice(bytes(raw, pos, g.start.max(pos)));
        let code = g.slot.unwrap_or(0);
        key.extend_from_slice(format!("{{{code:02x}}}").as_bytes());
        pos = g.end.max(pos);
        let content = normalize(bytes(raw, g.start, g.end).to_vec(), false);
        let filler = if content.is_empty() {
            // 空插槽之後的預設代名詞併入插槽（§8）。
            let rest = bytes(raw, pos, z);
            match PRONOUNS.iter().find(|(en, _)| {
                rest.starts_with(en.as_bytes()) && !rest.get(en.len()).is_some_and(u8::is_ascii_alphabetic)
            }) {
                Some((en, zh)) => {
                    pos += en.len() as u16;
                    ((*zh).to_string(), true)
                }
                None => (String::new(), true),
            }
        } else if let Some(zh) = PRONOUNS.iter().find(|(en, _)| en.as_bytes() == content.as_slice()) {
            (zh.1.to_string(), true)
        } else if let Some(zh) = lookup_with_punct(catalog, &content) {
            (zh, true)
        } else {
            compose_range(catalog, raw, segments, g.start, g.end, depth + 2)
        };
        fillers.push((code, filler));
    }
    key.extend_from_slice(bytes(raw, pos, z));
    let key = normalize(key, false);
    let Some(mut zh) = lookup_with_punct(catalog, &key) else {
        return (String::from_utf8_lossy(&plain).into_owned(), false);
    };
    let mut all = true;
    for (code, (text, found)) in fillers {
        all &= found;
        zh = zh.replacen(&format!("{{{code:02x}}}"), text.trim(), 1);
    }
    (zh, all)
}

/// 預設代名詞（§8）的中文。
const PRONOUNS: [(&str, &str); 3] = [("him", "他"), ("her", "她"), ("it", "它")];

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

    fn seg(start: u16, end: u16, depth: u8, slot: Option<u8>) -> Segment {
        Segment { start, end, depth, slot }
    }

    #[test]
    fn consecutive_phrases_are_joined() {
        let c = catalog(&[(b"One.", "一。"), (b"Two.", "二。")]);
        let raw = b"One. Two. ";
        assert_eq!(compose(&c, raw, &[seg(8, 13, 0, None), seg(13, 18, 0, None)]), Some(("一。二。".into(), true)));
        assert_eq!(compose(&c, raw, &[]), None, "沒有片語紀錄就不組合");
    }

    #[test]
    fn appended_punctuation_falls_back_to_the_stem() {
        // ex-office6：「You're wearing」「a」「KGB captain's uniform」之後，遊戲另補「.」。
        let c = catalog(&[(b"You're wearing", "你穿著"), (b"a", "一件"), (b"KGB captain's uniform", "KGB 上尉制服")]);
        let raw = b"You're wearing a KGB captain's uniform.";
        let segs = [seg(8, 23, 0, None), seg(23, 25, 0, None), seg(25, 47, 0, None)];
        assert_eq!(compose(&c, raw, &segs), Some(("你穿著一件 KGB 上尉制服。".into(), true)));
    }

    #[test]
    fn slot_template_is_filled_with_nested_phrase() {
        // ex-office4：片語 3380 在 FEh 插槽遞迴解碼片語 3339「yourself」，「.」屬外層片語。
        let c = catalog(&[(b"Inspect {fe}.", "檢視{fe}。"), (b"yourself", "你自己")]);
        let raw = b"Inspect yourself. ";
        let segs = [seg(16, 24, 2, None), seg(16, 24, 1, Some(0xfe)), seg(8, 26, 0, None)];
        assert_eq!(compose(&c, raw, &segs), Some(("檢視你自己。".into(), true)));
    }

    #[test]
    fn article_before_slot_name_is_dropped() {
        // ex-scan2：插槽內容為 5718 附加的「the 」加遞迴片語「portrait」。
        let c = catalog(&[(b"Inspect {fe}.", "檢視{fe}。"), (b"portrait", "肖像")]);
        let raw = b"Inspect the portrait. ";
        let segs = [seg(20, 29, 2, None), seg(16, 28, 1, Some(0xfe)), seg(8, 30, 0, None)];
        assert_eq!(compose(&c, raw, &segs), Some(("檢視肖像。".into(), true)));
    }

    #[test]
    fn empty_slot_takes_the_default_pronoun() {
        let c = catalog(&[(b"Talk to {fe}.", "和{fe}交談。")]);
        let raw = b"Talk to her. ";
        let segs = [seg(16, 16, 1, Some(0xfe)), seg(8, 21, 0, None)];
        assert_eq!(compose(&c, raw, &segs), Some(("和她交談。".into(), true)));
    }

    #[test]
    fn missing_template_keeps_english_and_reports_it() {
        let c = catalog(&[(b"yourself", "你自己")]);
        let raw = b"Inspect yourself. ";
        let segs = [seg(16, 24, 2, None), seg(16, 24, 1, Some(0xfe)), seg(8, 26, 0, None)];
        assert_eq!(compose(&c, raw, &segs), Some(("Inspect yourself.".into(), false)));
    }

    #[test]
    fn gaps_stay_english_and_missing_phrases_are_reported() {
        let c = catalog(&[(b"hello.", "你好。")]);
        let raw = b"- Ivan: hello. bye.";
        let (zh, all) = compose(&c, raw, &[seg(16, 23, 0, None), seg(23, 27, 0, None)]).unwrap();
        assert_eq!(zh, "- Ivan: 你好。 bye.");
        assert!(!all, "名稱與 bye. 沒有譯文");
    }

    #[test]
    fn submenu_arrow_is_kept_after_translation() {
        // ex-belov1：對話選單「You say  」＋7Fh（遊戲字型的右箭頭）。
        let c = catalog(&[(b"You say", "你說")]);
        let mut e = line_event(b"You say  \x7f");
        assert_eq!(shown(&c, &e), ("你說 ▶".into(), true));
        e.text = b"You shout  \x7f".to_vec();
        e.drawn = e.text.clone();
        assert_eq!(shown(&c, &e), ("You shout ▶".into(), false), "未譯時英文也換成箭頭");
    }

    fn line_event(text: &[u8]) -> TextEvent {
        TextEvent {
            step: 0,
            kind: crate::takeover::Kind::Line,
            site: 0x3e13,
            text: text.to_vec(),
            x: 0,
            y: 0,
            max_lines: 1,
            width: 0,
            line_height: 9,
            color: 0,
            font_seg: 0,
            taken: true,
            page: 0,
            full: text.to_vec(),
            drawn: text.to_vec(),
            block: (0, 0),
            raw: text.to_vec(),
            en_width: 40,
            centered: true,
            segments: Vec::new(),
        }
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
            en_width: 0,
            centered: false,
            segments: vec![seg(8, 13, 0, None), seg(13, 17, 0, None)],
        };
        assert_eq!(shown(&c, &e), ("二。".into(), true), "第二頁");
        e.page = 0;
        e.text = e.full.clone();
        assert_eq!(shown(&c, &e).0, "一。", "第一頁");
    }
}
