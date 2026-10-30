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
    /// 畫面寫入監看的範圍（預設 mode 13h 畫面，生命週期依此判斷，docs/spec/004 §5）。
    /// 實際交給匯流排與否由 `sync_watch` 決定；kgb-run 的 `--watch` 改寫這個值。
    pub watch: Option<(usize, usize)>,
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
    /// 已處理的開檔紀錄筆數（字卡用）。
    files_seen: usize,
    /// 圖塊文字覆繪（docs/spec/005）。
    pub sprites: Sprites,
    /// 取證用：從某步數起，依寫入指令的 CS:IP 彙整畫面寫入（次數、首次步數、位址範圍）。
    pub writers: Option<(u64, std::collections::BTreeMap<(u16, u16), (u64, u64, usize, usize)>)>,
    /// 取證用：進入這些 load segment 內偏移時記錄呼叫者（近返回位址）與暫存器。
    pub trace_at: Option<(Vec<u16>, String)>,
    /// 取證用：從某步數起記錄進入驅動程式跳躍表（驅動段 0100h–01FFh）的呼叫與暫存器。
    pub driver_log: Option<(u64, String)>,
    /// 取證用：第一次進入開章程式 `1bb9` 時把 AL 改成這一章，
    /// 只改暫存器，讓遊戲自己的開章程式載入該章（docs/re/006）。用一次即清除。
    pub start_chapter: Option<u8>,
    /// 取證用：過了這個步數後，第一次在腳本變數程式 `4d19` 推完 bx、dx、di（`4d1c`）時跳到
    /// `4f82`，即腳本值 F0h 的片尾路徑（`658b`、`231e`、`8924`，經 `50ff` 收尾，堆疊對稱）。
    /// 只改 IP（docs/spec/006 §4）。
    pub end_credits: Option<u64>,
    /// 片頭與片尾名單（docs/spec/006）。
    pub credits: crate::credits::Credits,
    /// 進行中的 `53d8`、`56d5` 呼叫。
    frames: Vec<Frame>,
    /// 各區塊（ES, DI）已完成的片語與插槽範圍（docs/spec/004 §7、§8）。
    segments: HashMap<(u16, u16), Vec<Segment>>,
    /// 會觸發接手或觀察的 load segment 內偏移（64K 位元表）。每步只查這張表，
    /// 命中、有進行中的片語呼叫、開了取證追蹤或圖塊快照到期時，才走完整檢查。
    hooks: Box<[u64; 1024]>,
    /// `sprites.next_due()` 的快取：每次 `observe` 後更新。
    sprites_due: u64,
}

/// 入口閘登記的偏移：片語、插槽、開章、片尾、圖塊、游標、名單、文字 routine。
fn hook_table() -> Box<[u64; 1024]> {
    let mut t = Box::new([0u64; 1024]);
    for ip in [
        PHRASE_DECODE,
        SLOT_FILL,
        START_CHAPTER,
        END_CREDITS_AT,
        crate::sprites::SPRITE_SELECT,
        crate::sprites::CURSOR_DRAW,
        crate::credits::CENTER_AT,
        crate::credits::LINE_AT,
        crate::takeover::BLOCK,
        crate::takeover::LINE,
        HOVER_ON,
        HOVER_OFF,
    ] {
        t[usize::from(ip >> 6)] |= 1 << (ip & 63);
    }
    t
}


/// 片語解碼器入口（docs/re/004）：AX＝片語偏移、DS＝PHZ 段、ES:DI＝文字區塊。
const PHRASE_DECODE: u16 = 0x53d8;
/// 插槽填入（docs/spec/004 §8）：AL＝字碼，ES:DI＝文字區塊。
const SLOT_FILL: u16 = 0x56d5;
/// 開章程式入口：AL＝章號（1–4），AH＝FFh；程式內把 AL 寫入 `[030e]`（`1bdf`）。
/// 新遊戲由 `18d6` 取 `[030e]` 後在 `18db` 呼叫（docs/re/006）。
const START_CHAPTER: u16 = 0x1bb9;
/// 腳本變數程式 `4d19` 推完 bx、dx、di 的位置（`end_credits` 用，docs/spec/006 §4）。
const END_CREDITS_AT: u16 = 0x4d1c;
/// 熱區換色（docs/spec/004 §10）：反白 `03df`、還原 `041b` 的 `lcall [05AAh]`；簽章各自從 `03dd`、`0417` 起，映像內各一處。
const HOVER_ON: u16 = 0x03df;
const HOVER_OFF: u16 = 0x041b;
const HOVER_ON_SIG: [u8; 6] = [0x8b, 0xc3, 0xff, 0x1e, 0xaa, 0x05];
const HOVER_OFF_SIG: [u8; 8] = [0x8b, 0xc3, 0x86, 0xc4, 0xff, 0x1e, 0xaa, 0x05];
/// mode 13h 畫面（320×200）的線性範圍。
const SCREEN_WATCH: (usize, usize) = (0xa0000, 0xaf9ff);

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
        Ok(Self {
            watch: Some(SCREEN_WATCH),
            m,
            take: Takeover::default(),
            screen: Screen::default(),
            catalog,
            log: String::from("step\tkind\tsite\tx\ty\tlines\tlh\tcolor\ttranslated\tkey\ttaken\tpage\tsegments\n"),
            events: 0,
            translated: 0,
            phrase_log: None,
            files_seen: 0,
            sprites: Sprites::default(),
            writers: None,
            driver_log: None,
            trace_at: None,
            start_chapter: None,
            end_credits: None,
            credits: Default::default(),
            frames: Vec::new(),
            segments: HashMap::new(),
            hooks: hook_table(),
            sprites_due: u64::MAX,
        })
    }

    /// 只在畫面寫入有人用時開寫入監看：有顯示中的覆繪項目、字卡在場或開了寫入者取證。
    /// 其他時候這些紀錄只會被空處理後清掉，關掉監看結果相同。
    fn sync_watch(&mut self) {
        let need = self.writers.is_some() || self.sprites.card_active() || !self.screen.is_empty();
        self.m.bus.write_watch = if need { self.watch } else { None };
    }

    /// 執行到步數達 `until` 或程式結束。
    pub fn run_until(&mut self, until: u64) -> Result<(), String> {
        self.sync_watch();
        while self.m.cpu.steps < until && !self.m.exited {
            let load = self.m.psp.wrapping_add(0x10);
            let ip = self.m.cpu.ip;
            let at_load = self.m.cpu.s[CS] == load;
            let hooked = at_load && self.hooks[usize::from(ip >> 6)] & (1 << (ip & 63)) != 0;
            if at_load
                && (hooked || !self.frames.is_empty() || self.trace_at.is_some() || self.m.cpu.steps >= self.sprites_due)
            {
                self.track_segments();
                if self.m.cpu.ip == START_CHAPTER {
                    if let Some(n) = self.start_chapter.take() {
                        self.m.cpu.set8(0, n);
                    }
                }
                if self.m.cpu.ip == END_CREDITS_AT && self.end_credits.is_some_and(|at| self.m.cpu.steps >= at) {
                    self.end_credits = None;
                    self.m.cpu.ip = 0x4f82;
                }
                self.hover();
                if !self.take.passthrough {
                    self.sprites.observe(&self.m);
                    self.sprites_due = self.sprites.next_due();
                    if self.credit_hook() {
                        continue;
                    }
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
            // 文字 routine 只在 BLOCK、LINE 兩個入口（已在表內）；IP 可能剛被上面改過，重新查表。
            let ip = self.m.cpu.ip;
            let text_hook = self.m.cpu.s[CS] == load && self.hooks[usize::from(ip >> 6)] & (1 << (ip & 63)) != 0;
            if let Some(mut e) = text_hook.then(|| self.take.intercept(&mut self.m)).flatten() {
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
                    self.sync_watch();
                }
                if taken {
                    continue;
                }
            }
            self.m.step()?;
            if self.m.file_log.len() != self.files_seen {
                for e in &self.m.file_log[self.files_seen.min(self.m.file_log.len())..] {
                    if e.ah == 0x3d && e.result >= 0 {
                        self.sprites.on_open(&e.name);
                    }
                }
                self.files_seen = self.m.file_log.len();
                self.sync_watch();
            }
            if !self.m.bus.write_hits.is_empty() {
                // 生命週期以虛擬時間分波（lifecycle::BURST_GAP）；選單列另比對 mode 13h 畫面內容（docs/spec/004 §5）。
                let at = self.m.bus.cycles;
                self.sprites.on_writes(at, &self.m.bus.write_hits);
                let chained = self.m.bus.vga.mode == 0x13 && !self.m.bus.vga.planar();
                let screen = chained.then(|| &self.m.bus.ram[0xa0000..0xa0000 + 64000]);
                self.screen.on_writes(at, &self.m.bus.write_hits, screen);
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
                self.m.bus.write_hits.clear();
                self.sync_watch();
            }
        }
        Ok(())
    }

    /// 熱區換色時改覆繪列的色號（docs/spec/004 §10）。
    /// 只讀，不改 CPU 與記憶體。CS 已確認是 load segment，簽章不符時不動作。
    fn hover(&mut self) {
        let c = &self.m.cpu;
        let (cs, ip) = (c.s[CS], c.ip);
        let code = |off: u16, sig: &[u8]| {
            sig.iter().enumerate().all(|(i, &b)| self.m.bus.ram[Cpu::address(cs, off.wrapping_add(i as u16)) & 0xfffff] == b)
        };
        let matched = match ip {
            HOVER_ON => code(HOVER_ON - 2, &HOVER_ON_SIG),
            HOVER_OFF => code(HOVER_OFF - 4, &HOVER_OFF_SIG),
            _ => false,
        };
        if !matched {
            return;
        }
        // DS:SI＝熱區記錄（x0, x1, y0, y1 各一字，含首尾）；驅動 0184h 把色號 AL 的像素換成 AH。
        let (ds, si) = (c.s[DS], c.r[ferrogolem_core::cpu::SI]);
        let w = |o: u16| usize::from(self.m.bus.word(Cpu::address(ds, si.wrapping_add(o))));
        let zone = (w(0), w(2), w(4), w(6));
        let (from, to) = (c.r[AX] as u8, (c.r[AX] >> 8) as u8);
        self.screen.on_recolor(zone, from, to);
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
    /// 名單（docs/spec/006）：`89e8` 記中心 x；`8a07` 讀本行、模擬 `pop dx`、跳到 `8a26`
    /// 略過英文繪字。改了 IP 時回傳 true。
    fn credit_hook(&mut self) -> bool {
        use crate::credits::{CENTER_AT, LINE_AT, NEXT_LINE, key, plain};
        use ferrogolem_core::cpu::{CX, DX, SI};
        let ip = self.m.cpu.ip;
        if ip == CENTER_AT {
            self.credits.center = self.m.cpu.r[DI];
            return false;
        }
        if ip != LINE_AT {
            return false;
        }
        let c = &self.m.cpu;
        let (ds, si, y) = (c.s[DS], c.r[SI], c.r[CX]);
        let mut en = Vec::new();
        for i in 0..200u16 {
            let b = self.m.bus.ram[Cpu::address(ds, si.wrapping_add(i))];
            if b < 0x20 {
                break;
            }
            en.push(b);
        }
        let (ss, sp) = (c.s[SS], c.r[SP]);
        let mode = self.m.bus.word(Cpu::address(ss, sp));
        self.m.cpu.r[DX] = mode;
        self.m.cpu.r[SP] = sp.wrapping_add(2);
        self.m.cpu.ip = NEXT_LINE;
        let k = key(&en);
        let (zh, found) = match self.catalog.get(&k) {
            Some(z) => (z.to_string(), true),
            None => (plain(&en), false),
        };
        self.events += 1;
        self.translated += u64::from(found);
        let _ = writeln!(
            self.log,
            "{}\tCredit\t{:04x}\t{}\t{}\t1\t14\t25\t{}\t{}\t1\t0\t{:04x}",
            self.m.cpu.steps, LINE_AT, self.credits.center, y, u8::from(found), key_of(&k), mode
        );
        self.credits.show(i32::from(y), zh, mode);
        true
    }

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
                let mut out = overlay::compose(&rgb, &self.screen.items(), &patches, &dac, g);
                if !self.take.passthrough && !self.credits.lines.is_empty() {
                    // 名單字色：英文名單完全淡入時金字主體的 RGB（docs/spec/006 §2）。
                    overlay::draw_credits(g, &mut out, &self.credits.lines, crate::credits::RGB);
                }
                (overlay::W, overlay::H, out)
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
    if let Some(r) = by_pattern(catalog, &e.text) {
        return r;
    }
    (String::from_utf8_lossy(&e.drawn).into_owned(), false)
}

/// 樣式列（docs/spec/004 §9）：執行期填入地點與數字的選單列。
fn by_pattern(catalog: &Catalog, text: &[u8]) -> Option<(String, bool)> {
    let text = String::from_utf8_lossy(text).replace(" ,", ",");
    catalog.patterns().iter().find_map(|(p, zh)| {
        let caps = crate::catalog::match_pattern(p, &text)?;
        let mut out = zh.clone();
        let mut all = true;
        for (i, cap) in caps.iter().enumerate() {
            let part = if cap.bytes().all(|b| b.is_ascii_digit()) {
                cap.clone()
            } else if let Some(z) = lookup_with_punct(catalog, cap.as_bytes()) {
                z
            } else {
                all = false;
                cap.clone()
            };
            out = out.replace(&format!("{{{i}}}"), &part);
        }
        Some((out, all))
    })
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
