// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
// RRSAL Go dosgolem VGA bus contract, f79eab6; no rust-dos implementation.
use super::cpu::Bus;
use std::collections::VecDeque;

pub struct Vga {
    pub planes: Vec<u8>,
    pub mode: u8,
    pub seq: [u8; 8],
    pub gc: [u8; 16],
    pub ac: [u8; 32],
    pub crtc: [u8; 32],
    pub dac: [[u8; 3]; 256],
    latch: [u8; 4],
    seq_index: usize,
    gc_index: usize,
    ac_index: usize,
    crtc_index: usize,
    ac_data: bool,
    misc: u8,
    dac_write: usize,
    dac_read: usize,
    dac_component: usize,
}
impl Default for Vga {
    fn default() -> Self {
        let mut v = Self {
            planes: vec![0; 4 * 65536],
            mode: 3,
            seq: [0; 8],
            gc: [0; 16],
            ac: [0; 32],
            crtc: [0; 32],
            dac: [[0; 3]; 256],
            latch: [0; 4],
            seq_index: 0,
            gc_index: 0,
            ac_index: 0,
            crtc_index: 0,
            ac_data: false,
            misc: 0xe3,
            dac_write: 0,
            dac_read: 0,
            dac_component: 0,
        };
        for i in 0..64 {
            let r = if i & 4 != 0 { 42 } else { 0 } + if i & 32 != 0 { 21 } else { 0 };
            let g = if i & 2 != 0 { 42 } else { 0 } + if i & 16 != 0 { 21 } else { 0 };
            let b = if i & 1 != 0 { 42 } else { 0 } + if i & 8 != 0 { 21 } else { 0 };
            v.dac[i] = [r, g, b];
        }
        v.reset(3);
        v
    }
}
impl Vga {
    pub fn planar(&self) -> bool {
        // mode 13h 清掉序列器鏈結位元（Mode X）時走平面（Go vga.go planarActive）。
        matches!(self.mode, 0x0d..=0x12) || (self.mode == 0x13 && self.seq[4] & 8 == 0)
    }
    pub fn reset(&mut self, mode: u8) {
        self.mode = mode;
        self.planes.fill(0);
        self.latch = [0; 4];
        self.seq = [0; 8];
        self.gc = [0; 16];
        self.ac = [0; 32];
        self.crtc = [0; 32];
        self.seq[2] = 15;
        self.gc[8] = 255;
        self.gc[7] = 15;
        self.crtc[0x17] = 0xe3;
        self.crtc[0x13] = 40;
        self.crtc[0x18] = 255;
        self.crtc[7] = 0x10;
        self.crtc[9] = 0x40;
        self.ac[..16].copy_from_slice(&[0, 1, 2, 3, 4, 5, 20, 7, 56, 57, 58, 59, 60, 61, 62, 63]);
        self.ac[0x12] = 15;
        self.seq_index = 0;
        self.gc_index = 0;
        self.ac_data = false;
        self.ac_index = 0;
        self.crtc_index = 0;
        if mode == 0x13 {
            // Go vga.go setMode13：鏈結位元立起、起點歸零、列距 28h。
            self.seq[4] = 0x0e;
            self.crtc[0x0c] = 0;
            self.crtc[0x0d] = 0;
            self.crtc[0x13] = 0x28;
            self.crtc[0x14] = 0x40;
            self.crtc[0x17] = 0xa3;
        }
    }
    /// mode 13h 畫面的 DAC 索引：鏈結時讀 A0000 線性記憶體，非鏈結時依 Go `Unchained256` 組四平面。
    pub fn indexed13(&self, linear: &[u8]) -> Vec<u8> {
        let (w, h) = (320usize, 200usize);
        let start = (usize::from(self.crtc[12]) << 8) | usize::from(self.crtc[13]);
        if self.seq[4] & 8 != 0 {
            return (0..w * h).map(|i| linear[(start * 4 + i) & 0xffff]).collect();
        }
        let pitch = if self.crtc[0x13] != 0 { usize::from(self.crtc[0x13]) * 2 } else { w / 4 };
        let mut out = vec![0; w * h];
        for y in 0..h {
            for x in 0..w {
                out[y * w + x] = self.planes[(x & 3) * 65536 + ((start + y * pitch + (x >> 2)) & 0xffff)];
            }
        }
        out
    }
    pub fn read(&mut self, off: usize) -> u8 {
        let off = off & 65535;
        for p in 0..4 {
            self.latch[p] = self.planes[p * 65536 + off]
        }
        if self.gc[5] & 8 == 0 {
            return self.latch[usize::from(self.gc[4] & 3)];
        }
        let mut out = 255;
        for p in 0..4 {
            if self.gc[7] & (1 << p) != 0 {
                out &= if self.gc[2] & (1 << p) != 0 {
                    self.latch[p]
                } else {
                    !self.latch[p]
                }
            }
        }
        out
    }
    pub fn write(&mut self, off: usize, data: u8) {
        let rotated = data.rotate_right(u32::from(self.gc[3] & 7));
        let mode = self.gc[5] & 3;
        let mask = if mode == 3 {
            self.gc[8] & rotated
        } else {
            self.gc[8]
        };
        for p in 0..4 {
            if self.seq[2] & (1 << p) == 0 {
                continue;
            }
            let latch = self.latch[p];
            let source = match mode {
                0 if self.gc[1] & (1 << p) == 0 => rotated,
                2 => {
                    if data & (1 << p) != 0 {
                        255
                    } else {
                        0
                    }
                }
                _ => {
                    if self.gc[0] & (1 << p) != 0 {
                        255
                    } else {
                        0
                    }
                }
            };
            let logic = match (self.gc[3] >> 3) & 3 {
                1 => source & latch,
                2 => source | latch,
                3 => source ^ latch,
                _ => source,
            };
            self.planes[p * 65536 + (off & 65535)] = if mode == 1 {
                latch
            } else {
                (logic & mask) | (latch & !mask)
            };
        }
    }
    pub fn output(&mut self, port: u16, value: u8) -> bool {
        match port {
            0x3c4 => self.seq_index = usize::from(value & 7),
            0x3c5 => self.seq[self.seq_index] = value,
            0x3ce => self.gc_index = usize::from(value & 15),
            0x3cf => self.gc[self.gc_index] = value,
            0x3d4 | 0x3b4 => self.crtc_index = usize::from(value & 31),
            0x3d5 | 0x3b5 => self.crtc[self.crtc_index] = value,
            0x3c0 => {
                if self.ac_data {
                    self.ac[self.ac_index] = value
                } else {
                    self.ac_index = usize::from(value & 31)
                }
                self.ac_data = !self.ac_data
            }
            0x3c2 => self.misc = value,
            0x3c6 => {}
            0x3c8 => {
                self.dac_write = usize::from(value);
                self.dac_component = 0
            }
            0x3c7 => {
                self.dac_read = usize::from(value);
                self.dac_component = 0
            }
            0x3c9 => {
                self.dac[self.dac_write][self.dac_component] = value & 63;
                self.dac_component += 1;
                if self.dac_component == 3 {
                    self.dac_component = 0;
                    self.dac_write = (self.dac_write + 1) & 255
                }
            }
            _ => return false,
        }
        true
    }
    pub fn input(&mut self, port: u16) -> Option<u8> {
        Some(match port {
            0x3c4 => self.seq_index as u8,
            0x3c5 => self.seq[self.seq_index],
            0x3ce => self.gc_index as u8,
            0x3cf => self.gc[self.gc_index],
            0x3d4 | 0x3b4 => self.crtc_index as u8,
            0x3d5 | 0x3b5 => self.crtc[self.crtc_index],
            0x3c1 => self.ac[self.ac_index],
            0x3cc => self.misc,
            0x3c6 => 255,
            0x3c8 => self.dac_write as u8,
            0x3c9 => {
                let v = self.dac[self.dac_read][self.dac_component];
                self.dac_component += 1;
                if self.dac_component == 3 {
                    self.dac_component = 0;
                    self.dac_read = (self.dac_read + 1) & 255
                }
                v
            }
            _ => return None,
        })
    }
    pub fn colors(&self) -> [[u8; 3]; 16] {
        std::array::from_fn(|i| {
            let mut index = self.ac[i] & 63;
            if self.ac[0x10] & 0x80 != 0 {
                index = (index & 15) | ((self.ac[0x14] & 3) << 4)
            }
            index |= (self.ac[0x14] & 12) << 4;
            self.dac[usize::from(index)].map(|x| (x << 2) | (x >> 4))
        })
    }
    pub fn rgb(&self) -> (usize, usize, Vec<u8>) {
        let (w, h) = match self.mode {
            0x0d => (320, 200),
            0x0e => (640, 200),
            0x0f | 0x10 => (640, 350),
            0x11 | 0x12 => (640, 480),
            _ => (640, 400),
        };
        let mut pixels = vec![0; w * h * 3];
        let palette = self.colors();
        let mut start = (usize::from(self.crtc[12]) << 8) | usize::from(self.crtc[13]);
        if self.crtc[0x17] & 0x40 == 0 {
            start *= 2
        }
        let pitch = if self.crtc[0x13] != 0 {
            usize::from(self.crtc[0x13]) * 2
        } else {
            w / 8
        };
        for y in 0..h {
            for x in 0..w {
                let offset = (start + y * pitch + x / 8) & 65535;
                let mut ink = 0;
                for p in 0..4 {
                    if self.planes[p * 65536 + offset] & (0x80 >> (x & 7)) != 0 {
                        ink |= 1 << p
                    }
                }
                pixels[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&palette[ink]);
            }
        }
        (w, h, pixels)
    }
}

pub struct Memory {
    pub ram: Vec<u8>,
    pub vga: Vga,
    pub cycles: u64,
    pub scans: VecDeque<u8>,
    pub port_error: Option<String>,
    pub pit_divisor: u32,
    pub sb: crate::sb::SbDsp,
    pub dma: crate::dma::Dma8237,
    /// 8259 主片 IMR（埠 21h）。
    pub pic_mask: u8,
    pub fdc_dor: u8,
    /// IRQ 7 掛起（SB 傳輸結束）。
    pub irq7_pending: bool,
    sb_transfer: Option<Transfer>,
    /// SB 8 位元 DMA 傳出的樣本，尚未混音；前端取走後清空。
    pub dac_out: Vec<u8>,
    pub dac_dropped: u64,
    pit_control: u8,
    pit_high: bool,
    pit_latch: u16,
    pit_latched: bool,
    pit_write: u16,
    port61: u8,
    opl_index: u8,
    opl_status: u8,
}
impl Default for Memory {
    fn default() -> Self {
        Self {
            ram: vec![0; 1 << 20],
            vga: Vga::default(),
            cycles: 0,
            scans: VecDeque::new(),
            port_error: None,
            pit_divisor: 65536,
            sb: crate::sb::SbDsp::with_dma(),
            dma: crate::dma::Dma8237::default(),
            pic_mask: 0,
            fdc_dor: 0x0c,
            irq7_pending: false,
            sb_transfer: None,
            dac_out: Vec::new(),
            dac_dropped: 0,
            pit_control: 0x36,
            pit_high: false,
            pit_latch: 0,
            pit_latched: false,
            pit_write: 0,
            port61: 0,
            opl_index: 0,
            opl_status: 0,
        }
    }
}
/// 種子的 CPU 週期時鐘：IRQ0 每 PIT 分頻 × 4 週期，PIT 為 1,193,182 Hz。
pub const CYCLES_PER_SECOND: u64 = 4 * 1_193_182;
const DAC_CAP: usize = 1 << 22;

/// SB 8 位元單次 DMA 傳輸（docs/spec/003 §3）。
struct Transfer {
    remaining: u32,
    next_at: u64,
    period: u64,
}

impl Memory {
    fn start_sb_transfer(&mut self, n: u32) {
        let (num, den) = self.sb.rate;
        let period = (CYCLES_PER_SECOND * den / num.max(1)).max(1);
        self.sb_transfer = Some(Transfer {
            remaining: n,
            next_at: self.cycles + period,
            period,
        });
    }
    /// 推進 SB DMA：到期就從通道 1 讀一個位元組。通道遮罩時暫停。傳完掛起 IRQ 7。
    pub fn tick_devices(&mut self) {
        while let Some(t) = &mut self.sb_transfer {
            if self.cycles < t.next_at || self.dma.mask & 2 != 0 {
                return;
            }
            t.next_at += t.period;
            t.remaining -= 1;
            let done = t.remaining == 0;
            let page = usize::from(self.dma.page[1].unwrap_or(0));
            let at = (page << 16) | usize::from(self.dma.current[2]);
            let v = self.ram[at & 0xfffff];
            self.dma.current[2] = self.dma.current[2].wrapping_add(1);
            self.dma.current[3] = self.dma.current[3].wrapping_sub(1);
            if self.dac_out.len() < DAC_CAP {
                self.dac_out.push(v)
            } else {
                self.dac_dropped += 1
            }
            if done {
                self.sb_transfer = None;
                self.sb.irq_pending = true;
                self.irq7_pending = true;
            }
        }
    }
    pub fn rgb(&self) -> (usize, usize, Vec<u8>) {
        if self.vga.mode == 0x13 {
            let idx = self.vga.indexed13(&self.ram[0xa0000..0xb0000]);
            let mut rgb = Vec::with_capacity(idx.len() * 3);
            for i in idx {
                rgb.extend(self.vga.dac[usize::from(i)].map(|x| (x << 2) | (x >> 4)));
            }
            return (320, 200, rgb);
        }
        if self.vga.mode != 3 {
            return self.vga.rgb();
        }
        let mut rgb = vec![0; 640 * 400 * 3];
        let palette = self.vga.colors();
        let base = 0xb8000 + usize::from(self.word(0x44e));
        for row in 0..25 {
            for col in 0..80 {
                let cell = base + (row * 80 + col) * 2;
                let ch = self.ram[cell];
                let attr = self.ram[cell + 1];
                for y in 0..16 {
                    let bits = if ch < 128 {
                        super::font::ASCII[usize::from(ch)][y / 2]
                    } else if matches!(ch, 0xc4 | 0xcd) {
                        if y / 2 == 3 { 255 } else { 0 }
                    } else if matches!(ch, 0xb3 | 0xba) {
                        0x18
                    } else {
                        0
                    };
                    for x in 0..8 {
                        let ink = if bits & (0x80 >> x) != 0 {
                            attr & 15
                        } else {
                            (attr >> 4) & 7
                        };
                        let i = ((row * 16 + y) * 640 + col * 8 + x) * 3;
                        rgb[i..i + 3].copy_from_slice(&palette[usize::from(ink)]);
                    }
                }
            }
        }
        (640, 400, rgb)
    }
    pub fn word(&self, a: usize) -> u16 {
        u16::from(self.ram[a & 0xfffff]) | (u16::from(self.ram[(a + 1) & 0xfffff]) << 8)
    }
    pub fn set_word(&mut self, a: usize, v: u16) {
        self.ram[a & 0xfffff] = v as u8;
        self.ram[(a + 1) & 0xfffff] = (v >> 8) as u8
    }
}
impl Bus for Memory {
    fn read(&mut self, a: usize) -> u8 {
        let a = a & 0xfffff;
        if (0xa0000..0xb0000).contains(&a) && self.vga.planar() {
            self.vga.read(a - 0xa0000)
        } else {
            self.ram[a]
        }
    }
    fn write(&mut self, a: usize, v: u8) {
        let a = a & 0xfffff;
        if (0xa0000..0xb0000).contains(&a) && self.vga.planar() {
            self.vga.write(a - 0xa0000, v)
        } else {
            self.ram[a] = v
        }
    }
    fn input(&mut self, p: u16) -> u8 {
        if let Some(v) = self.vga.input(p) {
            return v;
        }
        if (0x220..=0x22f).contains(&p) {
            if let Some(v) = self.sb.input(p) {
                return v;
            }
        }
        if crate::dma::Dma8237::handles(p) {
            if let Some(v) = self.dma.input(p) {
                return v;
            }
        }
        match p {
            0x3da | 0x3ba => {
                self.vga.ac_data = false;
                if self.cycles % 79545 > 73500 {
                    9
                } else {
                    ((self.cycles / 80) & 1) as u8
                }
            }
            0x40 => {
                let value = if self.pit_latched {
                    self.pit_latch
                } else {
                    (self.pit_divisor - ((self.cycles / 4) % u64::from(self.pit_divisor)) as u32)
                        as u16
                };
                let v = if self.pit_high {
                    (value >> 8) as u8
                } else {
                    value as u8
                };
                self.pit_high = !self.pit_high;
                if !self.pit_high {
                    self.pit_latched = false
                }
                v
            }
            0x61 => self.port61 | (((self.cycles / 400) & 1) as u8) << 5,
            0x60 => self.scans.pop_front().unwrap_or(0),
            0x64 => u8::from(!self.scans.is_empty()),
            0x388 => self.opl_status,
            0x389 => 255,
            0x21 => self.pic_mask,
            0x20 | 0xa0 | 0xa1 => 0,
            _ => {
                self.port_error = Some(format!("unsupported input port {p:04x}"));
                255
            }
        }
    }
    fn output(&mut self, p: u16, v: u8) {
        if self.vga.output(p, v) {
            return;
        }
        if (0x220..=0x22f).contains(&p) && self.sb.output(p, v) {
            if let Some(n) = self.sb.start8.take() {
                self.start_sb_transfer(n);
            }
            return;
        }
        if crate::dma::Dma8237::handles(p) && self.dma.output(p, v) {
            return;
        }
        match p {
            0x43 => {
                self.pit_control = v;
                self.pit_high = false;
                if v & 0xc0 == 0 && v & 0x30 == 0 {
                    self.pit_latch = (self.pit_divisor
                        - ((self.cycles / 4) % u64::from(self.pit_divisor)) as u32)
                        as u16;
                    self.pit_latched = true
                }
            }
            0x40 => {
                if self.pit_high {
                    self.pit_write = (self.pit_write & 255) | (u16::from(v) << 8);
                    self.pit_divisor = if self.pit_write == 0 {
                        65536
                    } else {
                        u32::from(self.pit_write)
                    }
                } else {
                    self.pit_write = u16::from(v)
                }
                self.pit_high = !self.pit_high
            }
            0x42 => {}
            0x61 => self.port61 = v,
            0x388 => self.opl_index = v,
            0x389 => {
                if self.opl_index == 4 {
                    if v & 0x80 != 0 {
                        self.opl_status = 0
                    } else if v & 1 != 0 {
                        self.opl_status = 0xc0
                    }
                }
            }
            0x21 => self.pic_mask = v,
            // 軟碟控制器 DOR：本機沒有軟碟，只保存寫入值（KGB 寫 0Ch 關馬達）。
            0x3f2 => self.fdc_dor = v,
            // 命令與 EOI 接受、不追蹤 in-service（docs/spec/003 §4）。
            0x20 | 0xa0 | 0xa1 => {}
            _ => self.port_error = Some(format!("unsupported output port {p:04x} value {v:02x}")),
        }
    }
}

#[cfg(test)]
mod transfer_tests {
    use super::*;

    fn program(m: &mut Memory, page: u8, addr: u16, len: u8) {
        m.output(0x0a, 5);
        m.output(0x0c, 0);
        m.output(0x0b, 0x49);
        m.output(0x02, addr as u8);
        m.output(0x02, (addr >> 8) as u8);
        m.output(0x03, len.wrapping_sub(1));
        m.output(0x03, 0);
        m.output(0x83, page);
        m.output(0x0a, 1);
        m.output(0x226, 1);
        m.output(0x226, 0);
        m.output(0x22c, 0x40);
        m.output(0x22c, 100);
        m.output(0x22c, 0x14);
        m.output(0x22c, len.wrapping_sub(1));
        m.output(0x22c, 0);
    }

    #[test]
    fn single_transfer_reads_bytes_then_raises_irq7() {
        let mut m = Memory::default();
        m.ram[0x517f1..0x517f4].copy_from_slice(&[7, 8, 9]);
        program(&mut m, 5, 0x17f1, 3);
        assert!(m.port_error.is_none());
        let period = CYCLES_PER_SECOND * 156 / 1_000_000;
        m.cycles += period * 2;
        m.tick_devices();
        assert_eq!(m.dac_out, vec![7, 8]);
        assert!(!m.irq7_pending, "還沒傳完不能發中斷");
        m.cycles += period;
        m.tick_devices();
        assert_eq!(m.dac_out, vec![7, 8, 9]);
        assert!(m.irq7_pending && m.sb.irq_pending);
        assert_eq!(m.dma.current[2], 0x17f4);
        assert_eq!(m.input(0x22a), 0xaa, "先取走 reset 回覆");
        assert_eq!(m.input(0x22e), 0);
        assert!(!m.sb.irq_pending, "讀 22Eh 清除 DSP 中斷");
    }

    #[test]
    fn masked_channel_pauses_transfer() {
        let mut m = Memory::default();
        program(&mut m, 5, 0x17f1, 1);
        m.output(0x0a, 5);
        m.cycles += CYCLES_PER_SECOND;
        m.tick_devices();
        assert!(m.dac_out.is_empty() && !m.irq7_pending);
        m.output(0x0a, 1);
        m.tick_devices();
        assert_eq!(m.dac_out.len(), 1);
        assert!(m.irq7_pending);
    }

    #[test]
    fn pic_mask_round_trip() {
        let mut m = Memory::default();
        m.output(0x21, 0x7f);
        assert_eq!(m.input(0x21), 0x7f);
        m.output(0x20, 0x20);
        assert!(m.port_error.is_none());
    }
}

#[cfg(test)]
mod mode13_tests {
    use super::*;

    #[test]
    fn chained_reads_linear_memory_and_dac() {
        let mut m = Memory::default();
        m.vga.reset(0x13);
        assert!(!m.vga.planar());
        m.write(0xa0000 + 321, 9);
        assert_eq!(m.ram[0xa0000 + 321], 9);
        m.output(0x3c8, 9);
        for c in [63, 0, 32] {
            m.output(0x3c9, c);
        }
        let (w, h, rgb) = m.rgb();
        assert_eq!((w, h), (320, 200));
        assert_eq!(&rgb[321 * 3..322 * 3], &[255, 0, 130]);
        assert_eq!(&rgb[0..3], &[0, 0, 0]);
    }

    #[test]
    fn clearing_chain4_switches_to_planes() {
        // 負對照：同一個 CPU 寫入，非鏈結時落進平面，不能出現在線性記憶體。
        let mut m = Memory::default();
        m.vga.reset(0x13);
        m.output(0x3c4, 4);
        m.output(0x3c5, 0x06);
        assert!(m.vga.planar());
        m.output(0x3c4, 2);
        m.output(0x3c5, 0x02); // 只寫平面 1
        m.write(0xa0000 + 5, 7);
        assert_eq!(m.ram[0xa0000 + 5], 0);
        let idx = m.vga.indexed13(&m.ram[0xa0000..0xb0000]);
        assert_eq!(idx[5 * 4 + 1], 7, "x=21 落在平面 1 第 5 格");
        assert_eq!(idx[5 * 4], 0);
    }
}
