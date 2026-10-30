// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 覆繪項目的生命週期（docs/spec/004 §5）。
//!
//! 原版每句先把字畫進離屏緩衝，再把整條字幕帶搬到螢幕一次（`51e0:0bcd`）；
//! 字幕一直留著，直到下一句的搬移或換場的整格重繪（`51e0:0c05`）蓋掉它。
//! 因此：事件後第一波寫到項目範圍的螢幕寫入算「上屏」，之後任何一波新的寫入就移除該項目。
//! 「一波」以虛擬時間區分：相鄰寫入相差超過 `BURST_GAP`（週期時鐘，0.1 虛擬秒）視為新的一波；
//! 以步數計時的話，時脈越快同樣的間隔步數越多，判斷會跟著時脈變。
//!
//! 對話選單列（呼叫點 `3e13` 的單行）改以內容判斷：游標每格重畫、移動時搬回背景、懸停換色都會
//! 寫到選單列，但選單還在；上屏後擷取該列畫面，之後的寫入讓畫面與快照的相同比例低於 85% 才移除。
//! 只處理 mode 13h 鏈結畫面（A0000 線性），其他模式的寫入位址不在此換算。
use crate::overlay::Item;
use crate::takeover::Kind;
use ferrogolem_core::memory::CYCLES_PER_SECOND;

/// 0.1 虛擬秒（週期時鐘單位）。校準時是 cycles 1000 的 100,000 步（kgb-r8）。
pub const BURST_GAP: u64 = CYCLES_PER_SECOND / 10;
const SCREEN: usize = 0xa0000;
/// 對話選單的呼叫點（docs/spec/004 §5、§10）。
pub const MENU_SITE: u16 = 0x3e13;
/// 選單列上屏後 50 虛擬毫秒內持續更新快照（整個選單的搬移可能分幾步完成）。
const SETTLE: u64 = CYCLES_PER_SECOND / 20;
/// 畫面與快照相同的比例低於此值才算換了內容；游標最多遮住一列約 7%。
const KEEP_PERCENT: usize = 85;

struct Live {
    item: Item,
    committed: bool,
    last_write: u64,
    /// 選單列：第一次上屏的時刻與該列畫面快照。
    first_write: u64,
    snap: Vec<u8>,
}

#[derive(Default)]
pub struct Screen {
    live: Vec<Live>,
    /// 被寫入移除的項目數（觀測用）。
    pub removed: u64,
}

/// 項目在 320×200 畫面上的範圍（含首、不含尾）：x0, y0, x1, y1。
pub fn rect(item: &Item) -> (usize, usize, usize, usize) {
    let e = &item.event;
    let lh = usize::from(e.line_height.max(1));
    let (x0, y0) = (usize::from(e.x).min(320), usize::from(e.y).min(200));
    match e.kind {
        Kind::Block => {
            let w = if e.width == 0 { 320 } else { usize::from(e.width) };
            let rows = usize::from(e.max_lines.max(1)) * lh + 1;
            (x0, y0, (x0 + w).min(320), (y0 + rows).min(200))
        }
        Kind::Line => (x0, y0, 320, (y0 + lh).min(200)),
    }
}

fn is_menu_row(item: &Item) -> bool {
    item.event.kind == Kind::Line && item.event.site == MENU_SITE
}

/// mode 13h 畫面（320×200 色號）上一個範圍的像素。
fn pixels(screen: &[u8], (x0, y0, x1, y1): (usize, usize, usize, usize)) -> Vec<u8> {
    (y0..y1).flat_map(|y| screen[y * 320 + x0..y * 320 + x1].iter().copied()).collect()
}

impl Screen {
    /// 新事件取代同一呼叫點、同一種類的舊項目；單行另要同一 y（選單逐行畫在同一呼叫點，ex-belov5）。
    pub fn on_event(&mut self, item: Item) {
        let e = &item.event;
        self.live.retain(|l| {
            let o = &l.item.event;
            o.site != e.site || o.kind != e.kind || (e.kind == Kind::Line && o.y != e.y)
        });
        self.live.push(Live {
            item,
            committed: false,
            last_write: 0,
            first_write: 0,
            snap: Vec::new(),
        });
    }

    /// 一步之內對 A0000 的寫入（線性位址）；`at` 為當時的週期時鐘（`bus.cycles`），
    /// `screen` 為寫入後的 mode 13h 畫面（64,000 個色號；不是鏈結畫面時為 None，選單列退回分波規則）。
    pub fn on_writes(&mut self, at: u64, hits: &[usize], screen: Option<&[u8]>) {
        let mut removed = 0;
        self.live.retain_mut(|l| {
            let r = rect(&l.item);
            let (x0, y0, x1, y1) = r;
            let touched = hits.iter().any(|&a| {
                let off = a.wrapping_sub(SCREEN);
                off < 64000 && (x0..x1).contains(&(off % 320)) && (y0..y1).contains(&(off / 320))
            });
            if !touched {
                return true;
            }
            if let (true, Some(scr)) = (is_menu_row(&l.item), screen.filter(|s| s.len() >= 64000)) {
                let now = pixels(scr, r);
                if !l.committed || at <= l.first_write + SETTLE {
                    if !l.committed {
                        (l.committed, l.first_write) = (true, at);
                    }
                    l.snap = now;
                    return true;
                }
                let same = now.iter().zip(&l.snap).filter(|(a, b)| a == b).count();
                if same * 100 >= now.len() * KEEP_PERCENT {
                    return true;
                }
                removed += 1;
                return false;
            }
            if l.committed && at > l.last_write + BURST_GAP {
                removed += 1;
                return false;
            }
            l.committed = true;
            l.last_write = at;
            true
        });
        self.removed += removed;
    }

    /// 熱區換色（docs/spec/004 §10），照原版驅動 0184h 的語意：熱區（x0..=x1, y0..=y1）內色號 `from`
    /// 的像素換成 `to`。覆繪的字以 `色號＋1` 畫（overlay::compose），所以與熱區相交、字色等於 `from`
    /// 的單行項目改為 `to − 1`。選單列由內容判斷存留；其他單行項目設回未上屏，
    /// 讓換色後的搬移算重新上屏而非新的一波。
    pub fn on_recolor(&mut self, zone: (usize, usize, usize, usize), from: u8, to: u8) {
        let (zx0, zx1, zy0, zy1) = zone;
        for l in &mut self.live {
            if l.item.event.kind != Kind::Line || l.item.event.color.wrapping_add(1) != from {
                continue;
            }
            let (x0, y0, x1, y1) = rect(&l.item);
            if x0 <= zx1 && zx0 < x1 && y0 <= zy1 && zy0 < y1 {
                l.item.event.color = to.wrapping_sub(1);
                if !is_menu_row(&l.item) {
                    l.committed = false;
                }
            }
        }
    }

    /// 換語言時重算顯示中項目的文字；生命週期狀態不變。
    pub fn retranslate(&mut self, f: impl Fn(&Item) -> String) {
        for l in &mut self.live {
            l.item.zh = f(&l.item);
        }
    }

    /// 清掉所有顯示中的項目（切到英文原版時用）。
    pub fn clear(&mut self) {
        self.live.clear();
    }

    /// 沒有顯示中的項目時，畫面寫入不會改變任何狀態。
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    pub fn items(&self) -> Vec<Item> {
        self.live.iter().map(|l| l.item.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::takeover::TextEvent;

    fn subtitle(step: u64) -> Item {
        Item {
            event: TextEvent {
                step,
                kind: Kind::Block,
                site: 0x2ce8,
                text: b"x".to_vec(),
                x: 0,
                y: 181,
                max_lines: 2,
                width: 320,
                line_height: 9,
                color: 254,
                font_seg: 0,
                taken: true,
                page: 0,
                full: b"x".to_vec(),
                drawn: b"x".to_vec(),
                block: (0, 0),
                raw: b"x".to_vec(),
                en_width: 0,
                centered: false,
                segments: Vec::new(),
            },
            zh: "字".into(),
        }
    }

    fn band_blit() -> Vec<usize> {
        (181 * 320..200 * 320).map(|o| SCREEN + o).collect()
    }

    #[test]
    fn commit_blit_keeps_then_scene_redraw_removes() {
        let mut s = Screen::default();
        assert_eq!(rect(&subtitle(0)), (0, 181, 320, 200));
        s.on_event(subtitle(100));
        s.on_writes(200, &band_blit(), None);
        s.on_writes(250, &band_blit(), None); // 同一波的後續寫入
        assert_eq!(s.items().len(), 1, "上屏那一波不能移除");
        s.on_writes(250 + BURST_GAP + 1, &[SCREEN + 190 * 320 + 5], None);
        assert!(s.items().is_empty(), "之後新的一波要移除");
        assert_eq!(s.removed, 1);
    }

    #[test]
    fn hover_recolor_highlights_row_and_survives_reblit() {
        // 對話選單一列（3e13，y 119，行高 10）與一段旁白（區塊）。
        let mut row = subtitle(0);
        (row.event.kind, row.event.site, row.event.x, row.event.y, row.event.line_height, row.event.color) =
            (Kind::Line, 0x3e13, 106, 119, 10, 66);
        let mut s = Screen::default();
        s.on_event(row);
        s.on_event(subtitle(0));
        // 畫面：選單底色 200；游標（16×14）與換成場景（色號 7）時的內容。
        let row_blit: Vec<usize> = (119 * 320..129 * 320).map(|o| SCREEN + o).collect();
        let mut screen = vec![200u8; 64000];
        s.on_writes(100, &row_blit, Some(&screen));
        s.on_recolor((100, 219, 119, 127), 0x43, 0xa1); // 反白：像素 43h→A1h
        let colors = |s: &Screen| s.items().iter().map(|i| (i.event.kind, i.event.color)).collect::<Vec<_>>();
        assert_eq!(colors(&s), vec![(Kind::Line, 0xa0), (Kind::Block, 254)], "只有單行項目反白，覆繪色號＝像素色號−1");
        // 很久之後游標壓到這列、懸停換色的搬移：內容幾乎不變，保留。
        for y in 119..129 {
            screen[y * 320 + 150..y * 320 + 166].fill(15);
        }
        s.on_writes(100 + 30 * BURST_GAP, &row_blit, Some(&screen));
        s.on_recolor((100, 219, 119, 127), 0xa1, 0x43); // 還原
        assert_eq!(colors(&s)[0], (Kind::Line, 66), "還原為事件原色");
        s.on_recolor((100, 219, 119, 127), 0x10, 0x20);
        assert_eq!(colors(&s)[0], (Kind::Line, 66), "負對照：字色不是被換的色號時不動");
        s.on_writes(100 + 60 * BURST_GAP, &row_blit, Some(&screen));
        assert_eq!(s.items().len(), 2, "游標與換色不移除選單列");
        // 選單關閉、這一列換成場景：移除；不受影響的旁白仍在。
        screen[119 * 320..129 * 320].fill(7);
        s.on_writes(100 + 60 * BURST_GAP + 1, &row_blit, Some(&screen));
        assert_eq!(colors(&s), vec![(Kind::Block, 254)]);
    }

    #[test]
    fn menu_row_snapshot_settles_during_upload() {
        // 負對照：上屏後 50 毫秒內選單還在搬（底色從 0 變 200），快照跟著更新，不能誤刪。
        let mut row = subtitle(0);
        (row.event.kind, row.event.site, row.event.x, row.event.y, row.event.line_height) = (Kind::Line, MENU_SITE, 106, 119, 10);
        let mut s = Screen::default();
        s.on_event(row);
        let row_blit: Vec<usize> = (119 * 320..129 * 320).map(|o| SCREEN + o).collect();
        let mut screen = vec![0u8; 64000];
        s.on_writes(1000, &row_blit, Some(&screen));
        screen[119 * 320..129 * 320].fill(200);
        s.on_writes(1000 + SETTLE / 2, &row_blit, Some(&screen));
        s.on_writes(1000 + 10 * SETTLE, &row_blit, Some(&screen));
        assert_eq!(s.items().len(), 1);
    }

    #[test]
    fn writes_outside_rect_do_nothing() {
        // 負對照：影片區（第 0–180 列）的寫入不影響字幕項目。
        let mut s = Screen::default();
        s.on_event(subtitle(100));
        s.on_writes(200, &band_blit(), None);
        s.on_writes(10_000_000, &[SCREEN + 180 * 320 + 319, SCREEN], None);
        assert_eq!(s.items().len(), 1);
    }

    #[test]
    fn new_event_replaces_same_site() {
        let mut s = Screen::default();
        s.on_event(subtitle(100));
        s.on_event(subtitle(200));
        assert_eq!(s.items().len(), 1);
        assert_eq!(s.items()[0].event.step, 200);
    }

    #[test]
    fn menu_lines_at_different_y_coexist() {
        // ex-belov5：對話選單四行都從 3e13 畫出，只有同一 y 的行互相取代。
        let line = |y: u16, step: u64| {
            let mut it = subtitle(step);
            (it.event.kind, it.event.site, it.event.y) = (Kind::Line, 0x3e13, y);
            it
        };
        let mut s = Screen::default();
        for (i, y) in [108, 119, 130, 141].into_iter().enumerate() {
            s.on_event(line(y, i as u64));
        }
        assert_eq!(s.items().len(), 4);
        s.on_event(line(119, 10));
        assert_eq!(s.items().len(), 4, "同一 y 取代");
    }
}
