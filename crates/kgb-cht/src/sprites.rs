// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 圖塊文字覆繪（docs/spec/005）：動作選單、物品選單與游標動詞是 SPRA.ANC 的圖塊，
//! 以 hook 記下遊戲畫了哪個圖塊、畫在哪裡，覆繪層蓋掉英文字、畫上中文。只讀，不寫入原版記憶體。
use crate::overlay::Patch;
use ferrogolem_core::cpu::{AX, BX, CS, CX, Cpu, DX};
use ferrogolem_core::machine::Machine;

const DATA: u16 = 0x2144 - 0x188b;
const DRIVER: u16 = 0x51e0 - 0x188b;
/// 畫圖塊（選取圖塊後交給驅動畫進離屏緩衝）。
pub(crate) const SPRITE_SELECT: u16 = 0x85f3;
const SPRITE_SIG: [u8; 10] = [0x1e, 0x53, 0x51, 0x8e, 0x06, 0xc0, 0x06, 0xbf, 0x00, 0x64];
/// 每格畫游標圖（`lcall [05ea]`），CX、BX＝左上角。
pub(crate) const CURSOR_DRAW: u16 = 0x7093;
const CURSOR_SIG: [u8; 4] = [0xff, 0x1e, 0xea, 0x05];
/// 圖塊畫進離屏緩衝所需的步數上限（ex-menu4：13,746 步內畫完）。
const DRAW_WINDOW: u64 = 40_000;
/// 快照與畫面相符的最低比例（百分比）。
const MATCH_PERCENT: usize = 85;

/// 標籤表的一列。
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub sprite: u16,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub en: String,
    pub zh_tw: String,
    pub zh_cn: String,
}

/// 讀 `text/sprite-labels.tsv`：八欄 圖塊、x、y、寬、高、英文、繁中、簡中；`#` 開頭為註解。
pub fn parse_labels(tsv: &str) -> Result<Vec<Label>, String> {
    let mut out = Vec::new();
    for (n, line) in tsv.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let c: Vec<&str> = line.split('\t').collect();
        if c.len() != 8 {
            return Err(format!("標籤表第 {} 行欄數 {}，應為 8", n + 1, c.len()));
        }
        let num = |s: &str| s.parse::<u16>().map_err(|e| format!("標籤表第 {} 行：{e}", n + 1));
        out.push(Label {
            sprite: num(c[0])?,
            x: num(c[1])?,
            y: num(c[2])?,
            w: num(c[3])?,
            h: num(c[4])?,
            en: c[5].into(),
            zh_tw: c[6].into(),
            zh_cn: c[7].into(),
        });
    }
    Ok(out)
}

/// 一次畫在畫面上的選單圖塊。
struct Placed {
    sprite: u16,
    x: i32,
    y: i32,
    step: u64,
    /// 各標籤矩形在離屏緩衝的色號快照（圖塊畫完後擷取）。
    snap: Option<Vec<Vec<u8>>>,
}

#[derive(Default)]
pub struct Sprites {
    labels: Vec<Label>,
    menus: Vec<Placed>,
    /// 最近一次畫游標：（步數, x, y, 圖塊編號）。
    cursor: Option<(u64, i32, i32, u16)>,
    /// 游標畫完後，各標籤矩形在畫面（A0000）的色號快照；滑鼠靜止時遊戲不重畫游標，靠它判斷游標還在（docs/spec/005 §4）。
    cursor_snap: Option<Vec<Vec<u8>>>,
    /// 開場字卡標籤（docs/spec/005 §5）。
    cards: Vec<Card>,
    /// 顯示中的字卡：（標籤索引, 是否已上屏, 最後一次寫入的步數）。
    card: Option<(usize, bool, u64)>,
    /// 簡中（否則繁中）。
    pub simplified: bool,
}

/// 字卡標籤表的一列：檔名與蓋住英文字的矩形（原版 320×200 座標）。
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub file: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub en: String,
    pub zh_tw: String,
    pub zh_cn: String,
}

/// 讀 `text/card-labels.tsv`：八欄 檔名、x、y、寬、高、英文、繁中、簡中。
pub fn parse_cards(tsv: &str) -> Result<Vec<Card>, String> {
    let mut out = Vec::new();
    for (n, line) in tsv.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let c: Vec<&str> = line.split('\t').collect();
        if c.len() != 8 {
            return Err(format!("字卡表第 {} 行欄數 {}，應為 8", n + 1, c.len()));
        }
        let num = |s: &str| s.parse::<i32>().map_err(|e| format!("字卡表第 {} 行：{e}", n + 1));
        out.push(Card {
            file: c[0].to_ascii_uppercase(),
            x: num(c[1])?,
            y: num(c[2])?,
            w: num(c[3])?,
            h: num(c[4])?,
            en: c[5].into(),
            zh_tw: c[6].into(),
            zh_cn: c[7].into(),
        });
    }
    Ok(out)
}

/// 相鄰寫入相差超過此步數視為新的一波（同 lifecycle::BURST_GAP）。
const CARD_GAP: u64 = crate::lifecycle::BURST_GAP;

fn signature(m: &Machine, cs: u16, ip: u16, sig: &[u8]) -> bool {
    sig.iter().enumerate().all(|(i, &b)| m.bus.ram[Cpu::address(cs, ip.wrapping_add(i as u16))] == b)
}

/// 矩形的底色與字色（色號）：底色為最常見的色號；字色從數量達非底色最多者 20% 的色號中，
/// 取與底色亮度差最大的一個（原版字有多階陰影，次常見的常是暗邊，ex-card4）。
pub fn bg_fg(px: &[u8], dac: &[[u8; 3]; 256]) -> (u8, u8) {
    let mut n = [0usize; 256];
    for &p in px {
        n[usize::from(p)] += 1;
    }
    let bg = (0..256).max_by_key(|&i| n[i]).unwrap_or(0);
    let luma = |i: usize| {
        let [r, g, b] = dac[i];
        u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114
    };
    let top = (0..256).filter(|&i| i != bg).map(|i| n[i]).max().unwrap_or(0);
    let fg = (0..256)
        .filter(|&i| i != bg && n[i] > 0 && n[i] * 10 >= top)
        .max_by_key(|&i| luma(i).abs_diff(luma(bg)));
    (bg as u8, fg.map_or(bg as u8 ^ 0xff, |i| i as u8))
}

/// 快照與目前畫面相同的比例是否達標。
pub fn matches(snap: &[u8], now: &[u8]) -> bool {
    let same = snap.iter().zip(now).filter(|(a, b)| a == b).count();
    !snap.is_empty() && same * 100 >= snap.len() * MATCH_PERCENT
}

/// 從 320 寬的色號平面取矩形（超出畫面的部分略去）。
fn rect(plane: &[u8], x: i32, y: i32, w: u16, h: u16) -> Vec<u8> {
    let mut v = Vec::with_capacity(usize::from(w) * usize::from(h));
    for yy in y..y + i32::from(h) {
        for xx in x..x + i32::from(w) {
            if (0..320).contains(&xx) && (0..200).contains(&yy) {
                v.push(plane[yy as usize * 320 + xx as usize]);
            }
        }
    }
    v
}

impl Sprites {
    pub fn new(labels: Vec<Label>) -> Self {
        Self { labels, ..Self::default() }
    }

    pub fn set_cards(&mut self, cards: Vec<Card>) {
        self.cards = cards;
    }

    /// 程式開檔（DOS 3Dh）。檔名是表上的字卡時開始等它上屏。
    pub fn on_open(&mut self, name: &str) {
        let base = name.rsplit(['\\', '/', ':']).next().unwrap_or(name).to_ascii_uppercase();
        if let Some(i) = self.cards.iter().position(|c| c.file == base) {
            self.card = Some((i, false, 0));
        }
    }

    /// 字卡在場（等待上屏或等待移除）；不在場時畫面寫入不影響字卡。
    pub fn card_active(&self) -> bool {
        self.card.is_some()
    }

    /// 一步之內對 A0000 的寫入：第一波寫到字卡矩形算上屏，之後新的一波就移除。
    pub fn on_writes(&mut self, step: u64, hits: &[usize]) {
        let Some((i, committed, last)) = self.card else { return };
        let c = &self.cards[i];
        let touched = hits.iter().any(|&a| {
            let off = a.wrapping_sub(0xa0000);
            let (x, y) = ((off % 320) as i32, (off / 320) as i32);
            off < 64000 && (c.x..c.x + c.w).contains(&x) && (c.y..c.y + c.h).contains(&y)
        });
        if !touched {
            return;
        }
        if committed && step > last + CARD_GAP {
            self.card = None;
        } else {
            self.card = Some((i, true, step));
        }
    }

    fn has(&self, sprite: u16) -> bool {
        self.labels.iter().any(|l| l.sprite == sprite)
    }

    /// 每步執行前呼叫（CS 已確認是 load segment）。只讀暫存器與記憶體。
    pub fn observe(&mut self, m: &Machine) {
        let (cs, ip, r) = (m.cpu.s[CS], m.cpu.ip, &m.cpu.r);
        let data = cs.wrapping_add(DATA);
        let word = |off: u16| m.bus.word(Cpu::address(data, off));
        if ip == SPRITE_SELECT && r[DX] == word(0x06d2) && self.has(r[AX]) && signature(m, cs, ip, &SPRITE_SIG) {
            self.menus.retain(|p| p.sprite != r[AX]);
            self.menus.push(Placed { sprite: r[AX], x: r[CX] as i16 as i32, y: r[BX] as i16 as i32, step: m.cpu.steps, snap: None });
        } else if ip == CURSOR_DRAW && signature(m, cs, ip, &CURSOR_SIG) {
            self.cursor = Some((m.cpu.steps, r[CX] as i16 as i32, r[BX] as i16 as i32, word(0x4f15)));
            self.cursor_snap = None;
        }
        if let (Some((at, x, y, sprite)), None) = (self.cursor, &self.cursor_snap) {
            if m.cpu.steps >= at + DRAW_WINDOW && m.bus.vga.mode == 0x13 {
                let screen = m.bus.vga.indexed13(&m.bus.ram[0xa0000..0xb0000]);
                let rects = self.labels.iter().filter(|l| l.sprite == sprite);
                self.cursor_snap = Some(rects.map(|l| rect(&screen, x + i32::from(l.x), y + i32::from(l.y), l.w, l.h)).collect());
            }
        }
        // 圖塊畫完後，從離屏緩衝擷取各標籤矩形當快照。
        let pending = self.menus.iter().any(|p| p.snap.is_none() && m.cpu.steps >= p.step + DRAW_WINDOW);
        if pending {
            let buffer = m.bus.word(Cpu::address(cs.wrapping_add(DRIVER), 0x020c));
            let base = Cpu::address(buffer, 0);
            let plane = &m.bus.ram[base..base + 64000];
            for p in &mut self.menus {
                if p.snap.is_none() && m.cpu.steps >= p.step + DRAW_WINDOW {
                    let labels = self.labels.iter().filter(|l| l.sprite == p.sprite);
                    p.snap = Some(labels.map(|l| rect(plane, p.x + i32::from(l.x), p.y + i32::from(l.y), l.w, l.h)).collect());
                }
            }
        }
    }

    /// `observe` 下一次需要做時間型檢查（游標與選單快照）的步數；沒有待辦時為 `u64::MAX`。
    /// 呼叫端在這之前只需在 `SPRITE_SELECT`、`CURSOR_DRAW` 呼叫 `observe`。
    pub fn next_due(&self) -> u64 {
        let cursor = match (self.cursor, &self.cursor_snap) {
            (Some((at, ..)), None) => at + DRAW_WINDOW,
            _ => u64::MAX,
        };
        self.menus.iter().filter(|p| p.snap.is_none()).map(|p| p.step + DRAW_WINDOW).fold(cursor, u64::min)
    }

    /// 依目前畫面（mode 13h 色號）算出要畫的標籤；已關閉的選單在此移除。
    pub fn patches(&mut self, indexed: &[u8], dac: &[[u8; 3]; 256], step: u64, ips: u64) -> Vec<Patch> {
        let simplified = self.simplified;
        let labels = &self.labels;
        let text = |l: &Label| if simplified { l.zh_cn.clone() } else { l.zh_tw.clone() };
        let mut out = Vec::new();
        self.menus.retain(|p| {
            let Some(snap) = &p.snap else { return true };
            let ls: Vec<&Label> = labels.iter().filter(|l| l.sprite == p.sprite).collect();
            let now: Vec<Vec<u8>> =
                ls.iter().map(|l| rect(indexed, p.x + i32::from(l.x), p.y + i32::from(l.y), l.w, l.h)).collect();
            if !snap.iter().zip(&now).all(|(s, n)| matches(s, n)) {
                return false;
            }
            // 同一個選單共用一組底色與字色，各按鈕的字色才一致（ex-spr4）。
            let (bg, fg) = bg_fg(&snap.concat(), dac);
            for l in &ls {
                out.push(Patch {
                    x: p.x + i32::from(l.x),
                    y: p.y + i32::from(l.y),
                    w: i32::from(l.w),
                    h: i32::from(l.h),
                    bg: dac[usize::from(bg)],
                    fg: dac[usize::from(fg)],
                    text: text(l),
                });
            }
            true
        });
        if let Some((i, true, _)) = self.card {
            let c = &self.cards[i];
            let (bg, fg) = bg_fg(&rect(indexed, c.x, c.y, c.w as u16, c.h as u16), dac);
            let text = if simplified { c.zh_cn.clone() } else { c.zh_tw.clone() };
            out.push(Patch { x: c.x, y: c.y, w: c.w, h: c.h, bg: dac[usize::from(bg)], fg: dac[usize::from(fg)], text });
        }
        if let Some((at, x, y, sprite)) = self.cursor {
            let ls: Vec<&Label> = labels.iter().filter(|l| l.sprite == sprite && (1..=11).contains(&sprite)).collect();
            let shown = match &self.cursor_snap {
                // 有快照：畫面上那塊還是原來的游標圖就一直畫（滑鼠靜止時遊戲不重畫）。
                Some(snap) => snap.iter().zip(&ls).all(|(s, l)| matches(s, &rect(indexed, x + i32::from(l.x), y + i32::from(l.y), l.w, l.h))),
                None => step.saturating_sub(at) <= ips / 10,
            };
            if shown && sprite >> 8 == 0 {
                for l in ls {
                    let (lx, ly) = (x + i32::from(l.x), y + i32::from(l.y));
                    let (bg, fg) = bg_fg(&rect(indexed, lx, ly, l.w, l.h), dac);
                    out.push(Patch { x: lx, y: ly, w: i32::from(l.w), h: i32::from(l.h), bg: dac[usize::from(bg)], fg: dac[usize::from(fg)], text: text(l) });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_parse_and_reject_bad_rows() {
        let l = parse_labels("# c\n16\t3\t5\t18\t7\tHIDE\t躲藏\t躲藏\n").unwrap();
        assert_eq!((l[0].sprite, l[0].x, l[0].w, l[0].zh_tw.as_str()), (16, 3, 18, "躲藏"));
        assert!(parse_labels("16\t3\t5\n").is_err(), "欄數不符要失敗");
    }

    #[test]
    fn card_commits_on_first_burst_and_leaves_on_the_next() {
        let mut s = Sprites::default();
        s.set_cards(parse_cards("TCD1A.SCR\t28\t84\t264\t24\tMoscow\t莫斯科\t莫斯科\n").unwrap());
        let inside = [0xa0000 + 90 * 320 + 100];
        s.on_writes(10, &inside);
        assert!(s.card.is_none(), "沒開檔前的寫入不算");
        s.on_open("D:\\TCD1A.SCR");
        s.on_writes(100, &[0xa0000]);
        assert_eq!(s.card, Some((0, false, 0)), "矩形外的寫入不算上屏");
        s.on_writes(200, &inside);
        s.on_writes(250, &inside);
        assert_eq!(s.card, Some((0, true, 250)), "同一波仍在");
        s.on_writes(250 + CARD_GAP + 1, &inside);
        assert!(s.card.is_none(), "新的一波（影片）蓋掉字卡");
        s.on_open("TCD9A.SCR");
        assert!(s.card.is_none(), "表上沒有的檔不理");
    }

    #[test]
    fn snapshot_match_tolerates_small_changes() {
        let snap = vec![1u8; 100];
        let mut now = snap.clone();
        assert!(matches(&snap, &now));
        now[..10].fill(9);
        assert!(matches(&snap, &now), "按鈕外框高亮只改到少數像素");
        now[..20].fill(9);
        assert!(!matches(&snap, &now), "選單關閉後大多不同");
        assert!(!matches(&[], &[]), "空快照不算相符");
    }

    #[test]
    fn text_color_is_the_frequent_color_with_most_contrast() {
        let mut dac = [[0u8; 3]; 256];
        dac[7] = [80, 40, 10]; // 暗邊
        dac[9] = [240, 160, 80]; // 亮字
        dac[11] = [255, 255, 255]; // 零星雜點
        let mut px = vec![0u8; 60];
        px.extend([7u8; 30]);
        px.extend([9u8; 10]);
        px.push(11);
        assert_eq!(bg_fg(&px, &dac), (0, 9), "取亮字，不取數量較多的暗邊，也不取零星雜點");
        assert_eq!(bg_fg(&[3, 3, 3], &dac), (3, 3 ^ 0xff), "只有一色時字色取反");
    }
}
