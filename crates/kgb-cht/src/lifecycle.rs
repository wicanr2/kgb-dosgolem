// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 覆繪項目的生命週期（docs/spec/004 §5）。
//!
//! 原版每句先把字畫進離屏緩衝，再把整條字幕帶搬到螢幕一次（`51e0:0bcd`）；
//! 字幕一直留著，直到下一句的搬移或換場的整格重繪（`51e0:0c05`）蓋掉它。
//! 因此：事件後第一波寫到項目範圍的螢幕寫入算「上屏」，之後任何一波新的寫入就移除該項目。
//! 「一波」以步數間隔區分：相鄰寫入相差超過 `BURST_GAP` 步視為新的一波。
//! 只處理 mode 13h 鏈結畫面（A0000 線性），其他模式的寫入位址不在此換算。
use crate::overlay::Item;
use crate::takeover::Kind;

pub const BURST_GAP: u64 = 100_000;
const SCREEN: usize = 0xa0000;

struct Live {
    item: Item,
    committed: bool,
    last_write: u64,
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

impl Screen {
    /// 新事件取代同一呼叫點、同一種類的舊項目。
    pub fn on_event(&mut self, item: Item) {
        self.live
            .retain(|l| l.item.event.site != item.event.site || l.item.event.kind != item.event.kind);
        self.live.push(Live {
            item,
            committed: false,
            last_write: 0,
        });
    }

    /// 一步之內對 A0000 的寫入（線性位址）。
    pub fn on_writes(&mut self, step: u64, hits: &[usize]) {
        let mut removed = 0;
        self.live.retain_mut(|l| {
            let (x0, y0, x1, y1) = rect(&l.item);
            let touched = hits.iter().any(|&a| {
                let off = a.wrapping_sub(SCREEN);
                off < 64000 && (x0..x1).contains(&(off % 320)) && (y0..y1).contains(&(off / 320))
            });
            if !touched {
                return true;
            }
            if l.committed && step > l.last_write + BURST_GAP {
                removed += 1;
                return false;
            }
            l.committed = true;
            l.last_write = step;
            true
        });
        self.removed += removed;
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
        s.on_writes(200, &band_blit());
        s.on_writes(250, &band_blit()); // 同一波的後續寫入
        assert_eq!(s.items().len(), 1, "上屏那一波不能移除");
        s.on_writes(250 + BURST_GAP + 1, &[SCREEN + 190 * 320 + 5]);
        assert!(s.items().is_empty(), "之後新的一波要移除");
        assert_eq!(s.removed, 1);
    }

    #[test]
    fn writes_outside_rect_do_nothing() {
        // 負對照：影片區（第 0–180 列）的寫入不影響字幕項目。
        let mut s = Screen::default();
        s.on_event(subtitle(100));
        s.on_writes(200, &band_blit());
        s.on_writes(10_000_000, &[SCREEN + 180 * 320 + 319, SCREEN]);
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
}
