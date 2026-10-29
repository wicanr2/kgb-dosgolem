// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 8237 DMA 控制器（第一顆，通道 0–3）的程式設定狀態。移植自 Go dosgolem f79eab6
//! `internal/machine/dma8237.go`。只保存設定，不執行資料傳輸。
pub struct Dma8237 {
    pub page: [Option<u8>; 4],
    /// 埠 0–7：偶數為通道位址、奇數為計數（Go 以埠號為索引）。
    pub base: [u16; 8],
    pub current: [u16; 8],
    known: [u8; 8],
    pub mode: [u8; 4],
    pub mask: u8,
    high: bool,
}

impl Default for Dma8237 {
    fn default() -> Self {
        Self {
            page: [None; 4],
            base: [0; 8],
            current: [0; 8],
            known: [0; 8],
            mode: [0; 4],
            mask: 15,
            high: false,
        }
    }
}

fn page_channel(port: u16) -> Option<usize> {
    match port {
        0x87 => Some(0),
        0x83 => Some(1),
        0x81 => Some(2),
        0x82 => Some(3),
        _ => None,
    }
}

impl Dma8237 {
    pub fn handles(port: u16) -> bool {
        port <= 0x0f || page_channel(port).is_some()
    }
    pub fn input(&mut self, port: u16) -> Option<u8> {
        if let Some(ch) = page_channel(port) {
            return self.page[ch];
        }
        if port > 7 {
            return None;
        }
        let (shift, bit) = if self.high { (8, 2) } else { (0, 1) };
        if self.known[usize::from(port)] & bit == 0 {
            return None;
        }
        self.high = !self.high;
        Some((self.current[usize::from(port)] >> shift) as u8)
    }
    pub fn output(&mut self, port: u16, v: u8) -> bool {
        if let Some(ch) = page_channel(port) {
            self.page[ch] = Some(v);
            return true;
        }
        if port <= 7 {
            let i = usize::from(port);
            let (shift, bit) = if self.high { (8, 2) } else { (0, 1) };
            self.current[i] = (self.current[i] & !(0xff << shift)) | (u16::from(v) << shift);
            self.base[i] = self.current[i];
            self.known[i] |= bit;
            self.high = !self.high;
            return true;
        }
        match port {
            0x0a => {
                let bit = 1 << (v & 3);
                if v & 4 != 0 {
                    self.mask |= bit
                } else {
                    self.mask &= !bit
                }
            }
            0x0b => self.mode[usize::from(v & 3)] = v & 0xfc,
            0x0c => self.high = false,
            0x0d => {
                self.high = false;
                self.mask = 15
            }
            0x0e => self.mask = 0,
            0x0f => self.mask = v & 15,
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel1_program_sequence() {
        let mut d = Dma8237::default();
        assert!(d.output(0x0a, 0x05));
        assert_eq!(d.mask, 15);
        assert!(d.output(0x0c, 0));
        assert!(d.output(0x0b, 0x49));
        assert_eq!(d.mode[1], 0x48);
        for (p, v) in [(2, 0x34), (2, 0x12), (3, 0xff), (3, 0x0f)] {
            assert!(d.output(p, v));
        }
        assert!(d.output(0x83, 0x05));
        assert_eq!((d.base[2], d.base[3], d.page[1]), (0x1234, 0x0fff, Some(5)));
        assert!(d.output(0x0a, 0x01));
        assert_eq!(d.mask, 13);
        assert!(d.output(0x0c, 0));
        assert_eq!((d.input(2), d.input(2)), (Some(0x34), Some(0x12)));
    }

    #[test]
    fn unknown_reads_and_ports_are_refused() {
        // 負對照：沒寫過的計數與非 8237 埠不能回假值。
        let mut d = Dma8237::default();
        assert_eq!(d.input(5), None);
        assert_eq!(d.input(0x81), None);
        assert!(!d.output(0x08, 0));
        assert!(!Dma8237::handles(0x80));
    }
}
