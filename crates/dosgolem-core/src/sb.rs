// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! Sound Blaster DSP 與 mixer 埠（基底 220h）。移植自 Go dosgolem f79eab6
//! `internal/machine/sb_dsp.go` 的公開埠契約受限子集，本庫規格 docs/spec/003-sound-blaster.md。
//!
//! DMA 尚未移植：需要 DMA 的命令（14h、1Ch、41h 之後的播放、B0h、C6h、D0h、D4h）
//! 記在 `dma_requests` 並回報不支援，由核心停在原處，不假裝播放成功。
#[derive(Clone, Debug, PartialEq)]
pub enum DmaRequest {
    Single8(u32),
    Auto8(u32),
    Auto16(u32, u8),
    Start16,
    Control8 { paused: bool },
}

#[derive(Default)]
pub struct SbDsp {
    pub speaker_on: bool,
    pub rate: (u64, u64),
    pub time_constant: Option<u8>,
    pub block_size: Option<u32>,
    pub irq_pending: bool,
    pub irq16_pending: bool,
    pub dma_requests: Vec<DmaRequest>,
    /// 核心已接上 8 位元單次 DMA 時為真；否則 DMA 命令一律拒絕。
    pub dma_enabled: bool,
    /// 已接受、待核心開始傳輸的 8 位元單次長度。
    pub start8: Option<u32>,
    pending: u8,
    length_low: u8,
    length_high_next: bool,
    b0_step: u8,
    c6_step: u8,
    c6_mode: u8,
    reset: bool,
    mixer_index: u8,
    /// mixer 暫存器檔。32h／33h 依 Go 只保留高 5 位元；其餘照寫入值保存（SB Pro 的 04h、22h、26h、28h 等）。
    mixer: Vec<Option<u8>>,
    reply: Vec<u8>,
}

impl SbDsp {
    /// 核心已接上 DMA 的 DSP（docs/spec/003 §2）。
    pub fn with_dma() -> Self {
        Self {
            dma_enabled: true,
            ..Self::default()
        }
    }
    fn mixer_reg(&self, i: u8) -> Option<u8> {
        self.mixer.get(usize::from(i)).copied().flatten()
    }
    pub fn input(&mut self, port: u16) -> Option<u8> {
        Some(match port {
            0x224 => self.mixer_index,
            0x225 => match self.mixer_index {
                0x32 | 0x33 => self.mixer_reg(self.mixer_index).unwrap_or(0xc0),
                0x82 => u8::from(self.irq_pending) | (u8::from(self.irq16_pending) << 1),
                0x80 => 4,
                0x81 => 0x22,
                i => return self.mixer_reg(i),
            },
            0x226 => 0xff,
            0x22e => {
                self.irq_pending = false;
                if self.reply.is_empty() { 0 } else { 0x80 }
            }
            0x22f => {
                self.irq16_pending = false;
                0
            }
            0x22a => {
                if self.reply.is_empty() {
                    0xff
                } else {
                    self.reply.remove(0)
                }
            }
            0x22c => {
                if self.reset {
                    0x80
                } else {
                    0
                }
            }
            _ => return None,
        })
    }

    /// 回傳 false 表示這個寫入不在已支援的契約內。
    pub fn output(&mut self, port: u16, v: u8) -> bool {
        if port == 0x224 {
            self.mixer_index = v;
            return true;
        }
        if port == 0x225 && !matches!(self.mixer_index, 0x80..=0x82) {
            let keep = if matches!(self.mixer_index, 0x32 | 0x33) { v & 0xf8 } else { v };
            if self.mixer.is_empty() {
                self.mixer = vec![None; 256];
            }
            self.mixer[usize::from(self.mixer_index)] = Some(keep);
            return true;
        }
        if port == 0x22c && !self.reset {
            if let Some(ok) = self.command(v) {
                return ok;
            }
        }
        if port == 0x22c && !self.reset && v == 0xe1 {
            self.reply = vec![4, 5];
            return true;
        }
        if port != 0x226 || v > 1 {
            return false;
        }
        if v == 1 {
            *self = Self {
                reset: true,
                rate: (22050, 1),
                mixer_index: self.mixer_index,
                mixer: std::mem::take(&mut self.mixer),
                dma_requests: std::mem::take(&mut self.dma_requests),
                dma_enabled: self.dma_enabled,
                ..Self::default()
            };
            return true;
        }
        if self.reset {
            self.reset = false;
            self.reply = vec![0xaa];
        }
        true
    }

    /// 22Ch 的命令位元組。None 表示不是這裡處理的命令，交給呼叫端的其餘規則。
    fn command(&mut self, v: u8) -> Option<bool> {
        match self.pending {
            0xc6 => {
                return Some(match self.c6_step {
                    0 if v & !0x30 == 0 => {
                        self.c6_mode = v;
                        self.c6_step = 1;
                        true
                    }
                    1 => {
                        self.length_low = v;
                        self.c6_step = 2;
                        true
                    }
                    2 => {
                        let n = u32::from(self.length_low) + u32::from(v) * 256 + 1;
                        self.dma_requests.push(DmaRequest::Auto16(n, self.c6_mode));
                        false
                    }
                    _ => false,
                });
            }
            0xb0 => {
                return Some(match (self.b0_step, v) {
                    (0, 0x30) | (1, 0) => {
                        self.b0_step += 1;
                        true
                    }
                    (2, 0) => {
                        self.dma_requests.push(DmaRequest::Start16);
                        false
                    }
                    _ => false,
                });
            }
            0x41 => {
                if !self.length_high_next {
                    self.length_low = v;
                    self.length_high_next = true;
                    return Some(true);
                }
                let rate = (u64::from(self.length_low) << 8) | u64::from(v);
                if !(5000..=45000).contains(&rate) {
                    return Some(false);
                }
                self.rate = (rate, 1);
                self.time_constant = None;
                self.pending = 0;
                self.length_high_next = false;
                return Some(true);
            }
            0x14 | 0x48 => {
                if !self.length_high_next {
                    self.length_low = v;
                    self.length_high_next = true;
                    return Some(true);
                }
                let n = u32::from(self.length_low) | (u32::from(v) << 8);
                if self.pending == 0x48 {
                    self.block_size = Some(n + 1);
                } else {
                    self.dma_requests.push(DmaRequest::Single8(n + 1));
                    if !self.dma_enabled {
                        return Some(false);
                    }
                    self.start8 = Some(n + 1);
                }
                self.pending = 0;
                self.length_high_next = false;
                return Some(true);
            }
            0xe0 => {
                // DSP 識別：回傳資料位元組的反相（DOSBox-X sblaster.cpp `case 0xe0`）。
                self.reply = vec![!v];
                self.pending = 0;
                return Some(true);
            }
            0x40 => {
                self.time_constant = Some(v);
                self.rate = (1_000_000, 256 - u64::from(v));
                self.pending = 0;
                return Some(true);
            }
            0 => {}
            _ => return None,
        }
        match v {
            0xc6 | 0xb0 => {
                self.pending = v;
                self.c6_step = 0;
                self.b0_step = 0;
                Some(true)
            }
            0xe0 => {
                self.pending = v;
                Some(true)
            }
            0x41 | 0x14 | 0x48 | 0x40 => {
                self.pending = v;
                self.length_high_next = false;
                Some(true)
            }
            0xd0 | 0xd4 => {
                self.dma_requests.push(DmaRequest::Control8 { paused: v == 0xd0 });
                Some(false)
            }
            0xd1 | 0xd3 => {
                self.speaker_on = v == 0xd1;
                Some(true)
            }
            0xd8 => {
                self.reply.push(if self.speaker_on { 0xff } else { 0 });
                Some(true)
            }
            0x1c => {
                if let Some(n) = self.block_size {
                    self.dma_requests.push(DmaRequest::Auto8(n));
                }
                Some(false)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_handshake_and_version() {
        let mut s = SbDsp::default();
        assert!(s.output(0x226, 1));
        assert_eq!(s.input(0x22c), Some(0x80), "reset 期間寫入埠忙碌");
        assert!(s.output(0x226, 0));
        assert_eq!(s.input(0x22e), Some(0x80));
        assert_eq!(s.input(0x22a), Some(0xaa));
        assert_eq!(s.input(0x22e), Some(0));
        assert!(s.output(0x22c, 0xe0));
        assert!(s.output(0x22c, 0x5a));
        assert_eq!(s.input(0x22a), Some(0xa5));
        assert!(s.output(0x22c, 0xe1));
        assert_eq!((s.input(0x22a), s.input(0x22a)), (Some(4), Some(5)));
        assert_eq!(s.input(0x22a), Some(0xff));
    }

    #[test]
    fn time_constant_and_speaker() {
        let mut s = SbDsp::default();
        s.output(0x226, 1);
        s.output(0x226, 0);
        assert!(s.output(0x22c, 0x40));
        assert!(s.output(0x22c, 0xa6));
        assert_eq!(s.time_constant, Some(0xa6));
        assert_eq!(s.rate, (1_000_000, 256 - 0xa6));
        assert!(s.output(0x22c, 0xd1));
        assert!(s.output(0x22c, 0xd8));
        s.input(0x22a); // 丟掉 reset 的 AAh
        assert_eq!(s.input(0x22a), Some(0xff));
    }

    #[test]
    fn dma_commands_are_refused_and_recorded() {
        // 負對照：DMA 未移植時，播放命令必須回不支援，不能靜默成功。
        let mut s = SbDsp::default();
        s.output(0x226, 1);
        s.output(0x226, 0);
        assert!(s.output(0x22c, 0x14));
        assert!(s.output(0x22c, 0xff));
        assert!(!s.output(0x22c, 0x0f));
        assert_eq!(s.dma_requests, vec![DmaRequest::Single8(0x1000)]);
        assert!(!s.output(0x22c, 0x99), "未知命令回不支援");
        assert!(!s.output(0x226, 2));
    }
}

#[cfg(test)]
mod dma_enabled_tests {
    use super::*;

    #[test]
    fn single8_is_accepted_when_core_has_dma() {
        let mut s = SbDsp {
            dma_enabled: true,
            ..SbDsp::default()
        };
        s.output(0x226, 1);
        assert!(s.dma_enabled, "reset 不清掉核心能力");
        s.output(0x226, 0);
        assert!(s.output(0x22c, 0x14));
        assert!(s.output(0x22c, 0x00));
        assert!(s.output(0x22c, 0x00));
        assert_eq!(s.start8, Some(1));
        assert!(!s.output(0x22c, 0x1c), "自動初始化仍未接上");
    }
}

#[cfg(test)]
mod mixer_tests {
    use super::*;

    #[test]
    fn sb_pro_mixer_round_trip() {
        let mut s = SbDsp::default();
        assert_eq!(s.input(0x225), None, "沒寫過的暫存器不回假值");
        s.output(0x224, 0x22);
        assert!(s.output(0x225, 0xcc));
        assert_eq!(s.input(0x225), Some(0xcc));
        s.output(0x224, 0x32);
        assert_eq!(s.input(0x225), Some(0xc0));
        s.output(0x225, 0xff);
        assert_eq!(s.input(0x225), Some(0xf8));
        s.output(0x224, 0x80);
        assert!(!s.output(0x225, 1), "IRQ 選擇暫存器唯讀");
        assert_eq!(s.input(0x225), Some(4));
    }
}
