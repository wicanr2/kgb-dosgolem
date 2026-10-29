// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 文字 routine 接手。偏移為 CSPPRG.EXE 解壓後 load segment 內的位置，
//! 每次攔截前比對 16 bytes 簽章（docs/spec/004 §2、§3）。
use dosgolem_core::cpu::{AX, BP, BX, CS, CX, Cpu, DI, DS, DX, ZF};
use dosgolem_core::machine::Machine;

pub const BLOCK: u16 = 0x86e9;
pub const LINE: u16 = 0x87d8;
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
}

#[derive(Default)]
pub struct Takeover {
    /// 簽章不符的次數（偏移, 次數）。不符時不攔截。
    pub mismatches: Vec<(u16, u64)>,
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
        let ss_sp = (m.cpu.s[dosgolem_core::cpu::SS], m.cpu.r[dosgolem_core::cpu::SP]);
        let site = word(m, ss_sp.0, ss_sp.1);
        let ds = m.cpu.s[DS];
        let (r, step) = (m.cpu.r, m.cpu.steps);
        let event = if ip == BLOCK {
            if !self.signature_ok(m, cs, ip, &BLOCK_SIG) {
                return None;
            }
            let di = r[DI];
            let end = word(m, ds, di.wrapping_add(2));
            let mut text: Vec<u8> = (r[BX]..end.max(r[BX]))
                .map(|i| byte(m, ds, di.wrapping_add(i)))
                .collect();
            // 86e9 把 0 與 `^` 都當換行；比對鍵去掉結尾的 0，中間的 0 寫成 `^`（docs/spec/004 §4）。
            while text.last() == Some(&0) {
                text.pop();
            }
            for b in &mut text {
                if *b == 0 {
                    *b = b'^'
                }
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
            // 87d8 的輸出：DI＝結尾字元之後、CX＝輸入 BX、DH＝[440e]、AL＝結尾字元（docs/spec/004 §2）。
            m.cpu.r[DI] = p;
            m.cpu.r[CX] = r[BX];
            m.cpu.r[DX] = (u16::from(color) << 8) | (r[DX] & 0xff);
            let ah = if text.is_empty() { r[AX] & 0xff00 } else { 0 };
            m.cpu.r[AX] = ah | u16::from(last);
            m.cpu.set_flag(ZF, true);
            TextEvent {
                step,
                kind: Kind::Line,
                site,
                text,
                x: r[CX],
                y: r[BX],
                max_lines: 1,
                width: 0,
                line_height: word(m, ds, 0x7691),
                color,
                font_seg: word(m, ds, 0x768f),
            }
        };
        m.cpu.ip = m.cpu.pop(&mut m.bus);
        Some(event)
    }
}
