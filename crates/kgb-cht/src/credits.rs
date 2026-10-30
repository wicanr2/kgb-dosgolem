// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 片頭與片尾名單（docs/spec/006）：名單程式逐行畫字前接手，跳過英文，譯文畫在覆繪層，
//! 透明度跟著原版每頁的淡入淡出重畫。
use crate::overlay::CreditLine;

/// 每行取得 x 之後：DI＝中心 x（腳本 x 為 0 時已設成 A0h）。
pub const CENTER_AT: u16 = 0x89e8;
/// 進入逐字迴圈前的 `pop dx`：DS:SI＝本行文字，CX＝y，堆疊頂＝模式值。
pub const LINE_AT: u16 = 0x8a07;
/// 找行尾、接下一行的入口。
pub const NEXT_LINE: u16 = 0x8a26;
/// 字色：英文名單金字主體（截圖 cheat2h，(203,158,85)–(219,199,125) 之間）。名單期間調色盤不變，
/// 淡入淡出由重畫完成，所以用固定色乘不透明度。
pub const RGB: [u8; 3] = [210, 172, 100];

/// 比對鍵：英文行連續空白縮成一個、去頭尾空白，加前綴 `credits:`（避免與章節片語撞 key）。
pub fn key(en: &[u8]) -> Vec<u8> {
    let mut out = b"credits:".to_vec();
    for w in en.split(|&b| b == b' ').filter(|w| !w.is_empty()) {
        if out.len() > 8 {
            out.push(b' ');
        }
        out.extend_from_slice(w);
    }
    out
}

/// 顯示用的英文原文（查不到譯文時）：同比對鍵的正規化，不含前綴。
pub fn plain(en: &[u8]) -> String {
    String::from_utf8_lossy(&key(en)[8..]).into_owned()
}

/// 模式值的不透明度：bit 15 為 1 是淡入（低位元組 10→0），為 0 是淡出（10→0，0 即消失）。
pub fn alpha(mode: u16) -> u8 {
    let level = u32::from(mode & 0xff).min(10);
    let a = if mode & 0x8000 != 0 { (11 - level) * 255 / 11 } else { level * 255 / 10 };
    a as u8
}

/// 目前顯示的名單行。
#[derive(Default)]
pub struct Credits {
    pub lines: Vec<CreditLine>,
    /// 最近一次 `CENTER_AT` 記下的中心 x。
    pub center: u16,
}

impl Credits {
    /// 同一 y 的新行取代舊行；不透明度為 0 即移除。
    pub fn show(&mut self, y: i32, text: String, mode: u16) {
        self.lines.retain(|l| l.y != y);
        let a = alpha(mode);
        if a > 0 {
            self.lines.push(CreditLine { cx: i32::from(self.center), y, text, alpha: a });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_collapses_spaces_and_adds_prefix() {
        assert_eq!(key(b"Background  graphics"), b"credits:Background graphics");
        assert_eq!(key(b"  THE     END "), b"credits:THE END");
        assert_eq!(plain(b"Michel  Rho"), "Michel Rho");
    }

    #[test]
    fn fade_in_rises_and_fade_out_falls() {
        // credits-r：每頁以 800a…8000 淡入、000a…0000 淡出。
        let ins: Vec<u8> = (0..=10).rev().step_by(2).map(|l| alpha(0x8000 | l)).collect();
        assert!(ins.windows(2).all(|w| w[0] < w[1]) && ins[5] == 255);
        let outs: Vec<u8> = (0..=10).rev().step_by(2).map(alpha).collect();
        assert!(outs.windows(2).all(|w| w[0] > w[1]) && outs[0] == 255 && outs[5] == 0);
    }

    #[test]
    fn same_y_replaces_and_zero_removes() {
        let mut c = Credits { center: 160, ..Default::default() };
        c.show(59, "出品".into(), 0x800a);
        c.show(59, "出品".into(), 0x8000);
        assert_eq!(c.lines.len(), 1);
        assert_eq!(c.lines[0].alpha, 255);
        c.show(88, "製作".into(), 0x8000);
        c.show(59, "出品".into(), 0x0000);
        assert_eq!(c.lines.iter().map(|l| l.y).collect::<Vec<_>>(), vec![88]);
    }
}
