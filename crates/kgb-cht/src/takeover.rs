// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 文字 routine 接手。偏移為 CSPPRG.EXE 解壓後 load segment 內的位置，
//! 每次攔截前比對 16 bytes 簽章（docs/spec/004 §2、§3）。
use ferrogolem_core::cpu::{AX, BP, BX, CS, CX, Cpu, DI, DS, DX, ZF};
use ferrogolem_core::machine::Machine;

pub const BLOCK: u16 = 0x86e9;
pub const LINE: u16 = 0x87d8;
/// 接手（跳過原版繪字）的呼叫點；其他呼叫點只記錄、原版照畫（docs/spec/004 §3）。
/// 2ce8 開場字幕、1d97 章節標題列、52a0 地點標籤、584f 旁白。時鐘 86b3 用特殊數字字型，不接手。
pub const TAKEN_SITES: [u16; 4] = [0x2ce8, 0x1d97, 0x52a0, 0x584f];
/// `86e9` 內部呼叫 `87d8` 後的返回偏移。
const BLOCK_INNER_RETURN: u16 = 0x8762;
const BLOCK_SIG: [u8; 16] = [
    0x50, 0x52, 0x56, 0x55, 0x06, 0x53, 0x51, 0x8d, 0x31, 0x8b, 0x4d, 0x02, 0x2b, 0xcb, 0xbb, 0x00,
];
const LINE_SIG: [u8; 16] = [
    0xfc, 0x55, 0x56, 0x06, 0x8a, 0x36, 0x0e, 0x44, 0x8b, 0xf7, 0x8b, 0xf9, 0x8b, 0xcb, 0xac, 0x3c,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Block,
    Line,
}

/// 一次被接手的繪字呼叫。座標是原版 320×200 畫面座標。
#[derive(Clone, Debug, PartialEq)]
pub struct TextEvent {
    pub step: u64,
    pub kind: Kind,
    /// 呼叫點：返回偏移（load segment 內）。
    pub site: u16,
    pub text: Vec<u8>,
    pub x: u16,
    pub y: u16,
    /// 區塊的最多行數（BP）；單行為 1。
    pub max_lines: u16,
    /// 區塊寬度 `[DI+4]`；單行為 0。
    pub width: u16,
    pub line_height: u16,
    pub color: u8,
    pub font_seg: u16,
    /// 真的跳過了原版繪字（呼叫點在 TAKEN_SITES）。為假時只記錄、CPU 不變。
    pub taken: bool,
    /// 區塊：頁序，`[8, BX)` 內 `^` 的個數；單行為 0（docs/spec/004 §4）。
    pub page: u16,
    /// 區塊：整塊原文 `[8, e)`，正規化同 `text`；單行同 `text`。
    pub full: Vec<u8>,
    /// 原版這次會畫的文字 `[BX, 返回 BX)`，正規化同 `text`，去掉結尾的 `^`。英文退路用。
    pub drawn: Vec<u8>,
    /// 區塊鍵（DS, DI）；單行為 (0, 0)。
    pub block: (u16, u16),
    /// 區塊：未正規化的整塊位元組 `[8, e)`；單行同 `text`。組合多片語譯文用。
    pub raw: Vec<u8>,
    /// 區塊內由 `53d8`（片語）與 `56d5`（插槽）寫入的範圍，由 Session 填入（docs/spec/004 §7、§8）。
    pub segments: Vec<Segment>,
}

/// 區塊內一段由片語解碼器或插槽寫入的範圍 `[start, end)`（區塊索引）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub start: u16,
    pub end: u16,
    /// 巢狀深度：最外層片語為 0，其插槽為 1，插槽內遞迴的片語為 2，依此類推。
    pub depth: u8,
    /// 插槽字碼（`56d5` 的 AL）；片語為 None。
    pub slot: Option<u8>,
}

/// 區塊資料的起點（`[DI+2]` 重設為 8：`2c40`、`7650`、`72cd`）。
pub const BLOCK_DATA: u16 = 8;

/// 比對鍵正規化：中間的 0 是 5bed 自動換行把空白改成的斷行，還原成空白；`^` 保留。
/// 去掉結尾的 0、空白與 `^`（片語每個字後補空白），比對鍵因此與區塊寬度無關（docs/spec/004 §4）。
pub fn normalize(mut text: Vec<u8>, trim_caret: bool) -> Vec<u8> {
    while matches!(text.last(), Some(&0) | Some(&b' ')) || (trim_caret && text.last() == Some(&b'^')) {
        text.pop();
    }
    for b in &mut text {
        if *b == 0 {
            *b = b' '
        }
    }
    text
}

/// `86e9` 返回的 BX（docs/spec/004 §3，`86f0`–`876c`）。`b(i)` 讀區塊索引 i 的位元組，
/// `end`＝`[DI+2]`，`bp`＝最多行數。算術照原版用 16 位元環繞。
pub fn block_advance(b: impl Fn(u16) -> u8, bx: u16, end: u16, bp: u16) -> u16 {
    let mut n: u16 = 0;
    if end > bx {
        let mut i = bx;
        let mut al;
        loop {
            al = b(i);
            i = i.wrapping_add(1);
            if al == b'^' {
                n = n.wrapping_add(1);
                break;
            }
            if al == 0 {
                n = n.wrapping_add(1);
            }
            if i == end {
                if al != 0 {
                    n = n.wrapping_add(1);
                }
                break;
            }
        }
        if al | b(i.wrapping_sub(2)) == 0 {
            n = n.wrapping_sub(1);
        }
        if bp < n {
            n = bp;
        }
    }
    let mut i = bx;
    for _ in 0..n {
        loop {
            let c = b(i);
            i = i.wrapping_add(1);
            if c == b'^' || c == 0 || i >= end {
                break;
            }
        }
    }
    i
}

#[derive(Default)]
pub struct Takeover {
    /// 簽章不符的次數（偏移, 次數）。不符時不攔截。
    pub mismatches: Vec<(u16, u64)>,
    /// 英文原版：只記錄事件，不跳過原版繪字（AGENTS.md §1「語言選單」）。
    pub passthrough: bool,
}

fn byte(m: &Machine, seg: u16, off: u16) -> u8 {
    m.bus.ram[Cpu::address(seg, off)]
}

fn word(m: &Machine, seg: u16, off: u16) -> u16 {
    u16::from(byte(m, seg, off)) | (u16::from(byte(m, seg, off.wrapping_add(1))) << 8)
}

impl Takeover {
    fn signature_ok(&mut self, m: &Machine, cs: u16, ip: u16, sig: &[u8; 16]) -> bool {
        let ok = (0..16u16).all(|i| byte(m, cs, ip + i) == sig[usize::from(i)]);
        if !ok {
            match self.mismatches.iter_mut().find(|(o, _)| *o == ip) {
                Some(e) => e.1 += 1,
                None => self.mismatches.push((ip, 1)),
            }
        }
        ok
    }

    /// 在下一道指令執行前呼叫。若它是文字 routine 入口，記下事件並模擬近返回。
    pub fn intercept(&mut self, m: &mut Machine) -> Option<TextEvent> {
        let (cs, ip) = (m.cpu.s[CS], m.cpu.ip);
        if cs != m.psp.wrapping_add(0x10) || (ip != BLOCK && ip != LINE) {
            return None;
        }
        let ss_sp = (m.cpu.s[ferrogolem_core::cpu::SS], m.cpu.r[ferrogolem_core::cpu::SP]);
        let site = word(m, ss_sp.0, ss_sp.1);
        let ds = m.cpu.s[DS];
        let (r, step) = (m.cpu.r, m.cpu.steps);
        let event = if ip == BLOCK {
            if !self.signature_ok(m, cs, ip, &BLOCK_SIG) {
                return None;
            }
            let di = r[DI];
            let end = word(m, ds, di.wrapping_add(2));
            let at = |i: u16| byte(m, ds, di.wrapping_add(i));
            let span = |a: u16, z: u16| (a..z.max(a)).map(at).collect::<Vec<u8>>();
            let advance = block_advance(at, r[BX], end, r[BP]);
            let start = BLOCK_DATA.min(r[BX]);
            let page = span(start, r[BX]).iter().filter(|&&c| c == b'^').count() as u16;
            let raw = span(start, end);
            let (text, full, drawn) = (
                normalize(span(r[BX], end), false),
                normalize(raw.clone(), false),
                normalize(span(r[BX], advance), true),
            );
            let taken = !self.passthrough && TAKEN_SITES.contains(&site);
            if taken {
                m.cpu.r[BX] = advance;
            }
            TextEvent {
                step,
                kind: Kind::Block,
                site,
                text,
                x: r[CX],
                y: r[DX],
                max_lines: r[BP],
                width: word(m, ds, di.wrapping_add(4)),
                line_height: word(m, ds, 0x7691),
                color: byte(m, ds, 0x440e),
                font_seg: word(m, ds, 0x768f),
                taken,
                page,
                full,
                drawn,
                block: (ds, di),
                raw,
                segments: Vec::new(),
            }
        } else {
            if site == BLOCK_INNER_RETURN || !self.signature_ok(m, cs, ip, &LINE_SIG) {
                return None;
            }
            let mut text = Vec::new();
            let mut p = r[DI];
            let last = loop {
                let b = byte(m, ds, p);
                p = p.wrapping_add(1);
                if b == 0 || b == b'^' || text.len() > 512 {
                    break b;
                }
                text.push(b);
            };
            let color = byte(m, ds, 0x440e);
            let taken = !self.passthrough && TAKEN_SITES.contains(&site);
            if taken {
            // 87d8 的輸出：DI＝結尾字元之後、CX＝輸入 BX、DH＝[440e]、AL＝結尾字元（docs/spec/004 §2）。
            m.cpu.r[DI] = p;
            m.cpu.r[CX] = r[BX];
            m.cpu.r[DX] = (u16::from(color) << 8) | (r[DX] & 0xff);
            let ah = if text.is_empty() { r[AX] & 0xff00 } else { 0 };
            m.cpu.r[AX] = ah | u16::from(last);
            m.cpu.set_flag(ZF, true);
            }
            TextEvent {
                step,
                kind: Kind::Line,
                site,
                x: r[CX],
                y: r[BX],
                max_lines: 1,
                width: 0,
                line_height: word(m, ds, 0x7691),
                color,
                font_seg: word(m, ds, 0x768f),
                taken,
                page: 0,
                full: text.clone(),
                drawn: text.clone(),
                block: (0, 0),
                raw: text.clone(),
                segments: Vec::new(),
                text,
            }
        };
        if event.taken {
            m.cpu.ip = m.cpu.pop(&mut m.bus);
        }
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 區塊資料放在索引 8 起，回傳讀取函式與結尾索引。
    fn block(data: &[u8]) -> (impl Fn(u16) -> u8 + '_, u16) {
        let at = move |i: u16| if i >= 8 { data.get(usize::from(i - 8)).copied().unwrap_or(0) } else { 0x20 };
        (at, 8 + data.len() as u16)
    }

    #[test]
    fn caret_ends_the_page() {
        let (b, end) = block(b"Hello^world");
        assert_eq!(block_advance(&b, 8, end, 5), 14, "第一頁畫到 ^ 為止，BX 越過 ^");
        assert_eq!(block_advance(&b, 14, end, 5), end, "第二頁畫到結尾");
    }

    #[test]
    fn lines_beyond_bp_are_left_for_the_next_call() {
        // 0 是自動換行的行界；三行但 BP＝2，只畫兩行。
        let (b, end) = block(b"aa\0bb\0cc");
        assert_eq!(block_advance(&b, 8, end, 2), 14);
        assert_eq!(block_advance(&b, 8, end, 3), end);
    }

    #[test]
    fn trailing_zeros_and_empty_blocks() {
        let (b, end) = block(b"ab\0");
        assert_eq!(block_advance(&b, 8, end, 5), 11);
        let (b, end) = block(b"ab\0\0");
        assert_eq!(block_advance(&b, 8, end, 5), 11, "結尾兩個 0 不多算一行");
        let (b, end) = block(b"");
        assert_eq!(block_advance(&b, 8, end, 5), 8, "空區塊 BX 不變");
    }

    #[test]
    fn normalize_restores_wrap_spaces() {
        assert_eq!(normalize(b"series of\0inc \0".to_vec(), false), b"series of inc");
        assert_eq!(normalize(b"page one ^".to_vec(), true), b"page one");
        assert_eq!(normalize(b"a^b^".to_vec(), false), b"a^b^", "比對鍵保留 ^");
    }
}
