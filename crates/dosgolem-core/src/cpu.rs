// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
// Rust port of wicanr2/dosgolem internal/cpu at f79eab6.
// Copyright 2026 Wang Chun-Yu (wicanr2). Original source:
// https://github.com/wicanr2/dosgolem ; see docs/native-core.md.
pub const AX: usize = 0;
pub const CX: usize = 1;
pub const DX: usize = 2;
pub const BX: usize = 3;
pub const SP: usize = 4;
pub const BP: usize = 5;
pub const SI: usize = 6;
pub const DI: usize = 7;
pub const ES: usize = 0;
pub const CS: usize = 1;
pub const SS: usize = 2;
pub const DS: usize = 3;
pub const CF: u16 = 1;
pub const PF: u16 = 4;
pub const AF: u16 = 0x10;
pub const ZF: u16 = 0x40;
pub const SF: u16 = 0x80;
pub const TF: u16 = 0x100;
pub const IF: u16 = 0x200;
pub const DF: u16 = 0x400;
pub const OF: u16 = 0x800;
#[path = "cpu_ops.rs"]
mod ops;

pub trait Bus {
    fn read(&mut self, address: usize) -> u8;
    fn write(&mut self, address: usize, value: u8);
    fn input(&mut self, port: u16) -> u8;
    fn output(&mut self, port: u16, value: u8);
}

#[derive(Clone, Debug)]
pub struct Cpu {
    pub r: [u16; 8],
    pub s: [u16; 4],
    pub ip: u16,
    pub flags: u16,
    pub halted: bool,
    pub model186: bool,
    pub steps: u64,
    pub cycles: u64,
    pub start: (u16, u16),
    segment: Option<usize>,
    rep: u8,
    fpu_control: u16,
}
impl Default for Cpu {
    fn default() -> Self {
        Self {
            r: [0; 8],
            s: [0, 0xffff, 0, 0],
            ip: 0,
            flags: 0xf002,
            halted: false,
            model186: true,
            steps: 0,
            cycles: 0,
            start: (0, 0),
            segment: None,
            rep: 0,
            fpu_control: 0,
        }
    }
}
#[derive(Clone, Copy)]
struct Operand {
    reg: Option<usize>,
    seg: u16,
    off: u16,
}
impl Operand {
    fn register(i: usize) -> Self {
        Self {
            reg: Some(i),
            seg: 0,
            off: 0,
        }
    }
}

impl Cpu {
    pub fn flag(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }
    pub fn set_flag(&mut self, flag: u16, on: bool) {
        if on {
            self.flags |= flag
        } else {
            self.flags &= !flag
        }
    }
    pub fn set_flags(&mut self, flags: u16) {
        self.flags = (flags & 0x0fd5) | 0xf002;
    }
    pub fn reg8(&self, r: usize) -> u8 {
        if r < 4 {
            self.r[r] as u8
        } else {
            (self.r[r - 4] >> 8) as u8
        }
    }
    pub fn set8(&mut self, r: usize, value: u8) {
        if r < 4 {
            self.r[r] = (self.r[r] & 0xff00) | u16::from(value)
        } else {
            self.r[r - 4] = (self.r[r - 4] & 255) | (u16::from(value) << 8)
        }
    }
    pub fn address(seg: u16, off: u16) -> usize {
        ((usize::from(seg) << 4) + usize::from(off)) & 0xfffff
    }
    pub fn read16<B: Bus>(bus: &mut B, seg: u16, off: u16) -> u16 {
        u16::from(bus.read(Self::address(seg, off)))
            | (u16::from(bus.read(Self::address(seg, off.wrapping_add(1)))) << 8)
    }
    pub fn write16<B: Bus>(bus: &mut B, seg: u16, off: u16, value: u16) {
        bus.write(Self::address(seg, off), value as u8);
        bus.write(Self::address(seg, off.wrapping_add(1)), (value >> 8) as u8);
    }
    fn fetch<B: Bus>(&mut self, bus: &mut B) -> u8 {
        let byte = bus.read(Self::address(self.s[CS], self.ip));
        self.ip = self.ip.wrapping_add(1);
        byte
    }
    fn immediate<B: Bus>(&mut self, bus: &mut B, wide: bool) -> u16 {
        let lo = u16::from(self.fetch(bus));
        if wide {
            lo | (u16::from(self.fetch(bus)) << 8)
        } else {
            lo
        }
    }
    fn data(&self, default: usize) -> u16 {
        self.s[self.segment.unwrap_or(default)]
    }
    pub fn push<B: Bus>(&mut self, bus: &mut B, v: u16) {
        self.cycles += 2;
        self.r[SP] = self.r[SP].wrapping_sub(2);
        Self::write16(bus, self.s[SS], self.r[SP], v);
    }
    pub fn pop<B: Bus>(&mut self, bus: &mut B) -> u16 {
        self.cycles += 2;
        let v = Self::read16(bus, self.s[SS], self.r[SP]);
        self.r[SP] = self.r[SP].wrapping_add(2);
        v
    }
    pub fn interrupt<B: Bus>(&mut self, bus: &mut B, n: u8) {
        self.cycles += 40;
        self.push(bus, self.flags);
        self.set_flag(IF | TF, false);
        self.push(bus, self.s[CS]);
        self.push(bus, self.ip);
        self.ip = Self::read16(bus, 0, u16::from(n) * 4);
        self.s[CS] = Self::read16(bus, 0, u16::from(n) * 4 + 2);
        self.halted = false;
    }
    fn decode<B: Bus>(&mut self, bus: &mut B) -> (usize, Operand) {
        let b = self.fetch(bus);
        let mode = b >> 6;
        let reg = usize::from((b >> 3) & 7);
        let rm = usize::from(b & 7);
        if mode == 3 {
            return (reg, Operand::register(rm));
        }
        self.cycles += 4;
        let (mut off, def) = match rm {
            0 => (self.r[BX].wrapping_add(self.r[SI]), DS),
            1 => (self.r[BX].wrapping_add(self.r[DI]), DS),
            2 => (self.r[BP].wrapping_add(self.r[SI]), SS),
            3 => (self.r[BP].wrapping_add(self.r[DI]), SS),
            4 => (self.r[SI], DS),
            5 => (self.r[DI], DS),
            6 if mode == 0 => (self.immediate(bus, true), DS),
            6 => (self.r[BP], SS),
            _ => (self.r[BX], DS),
        };
        if mode == 1 {
            off = off.wrapping_add(self.fetch(bus) as i8 as i16 as u16)
        } else if mode == 2 {
            off = off.wrapping_add(self.immediate(bus, true))
        }
        (
            reg,
            Operand {
                reg: None,
                seg: self.data(def),
                off,
            },
        )
    }
    fn get<B: Bus>(&self, bus: &mut B, o: Operand, wide: bool) -> u16 {
        match o.reg {
            Some(i) => {
                if wide {
                    self.r[i]
                } else {
                    u16::from(self.reg8(i))
                }
            }
            None => {
                if wide {
                    Self::read16(bus, o.seg, o.off)
                } else {
                    u16::from(bus.read(Self::address(o.seg, o.off)))
                }
            }
        }
    }
    fn put<B: Bus>(&mut self, bus: &mut B, o: Operand, wide: bool, v: u16) {
        match o.reg {
            Some(i) => {
                if wide {
                    self.r[i] = v
                } else {
                    self.set8(i, v as u8)
                }
            }
            None => {
                if wide {
                    Self::write16(bus, o.seg, o.off, v)
                } else {
                    bus.write(Self::address(o.seg, o.off), v as u8)
                }
            }
        }
    }
    fn szp(&mut self, v: u16, wide: bool) {
        let v = if wide { v } else { v & 255 };
        self.set_flag(SF, v & if wide { 0x8000 } else { 0x80 } != 0);
        self.set_flag(ZF, v == 0);
        self.set_flag(PF, (v as u8).count_ones().is_multiple_of(2));
    }
    fn add(&mut self, a: u16, b: u16, carry: u16, wide: bool) -> u16 {
        let mask = if wide { 65535 } else { 255 };
        let sign = if wide { 0x8000 } else { 0x80 };
        let full = u32::from(a) + u32::from(b) + u32::from(carry);
        let r = (full & mask) as u16;
        self.set_flag(CF, full > mask);
        self.set_flag(AF, (a & 15) + (b & 15) + carry > 15);
        self.set_flag(OF, (a ^ r) & (b ^ r) & sign != 0);
        self.szp(r, wide);
        r
    }
    fn sub(&mut self, a: u16, b: u16, carry: u16, wide: bool) -> u16 {
        let mask = if wide { 65535 } else { 255 };
        let sign = if wide { 0x8000 } else { 0x80 };
        let r = a.wrapping_sub(b).wrapping_sub(carry) & mask;
        self.set_flag(CF, u32::from(a) < u32::from(b) + u32::from(carry));
        self.set_flag(AF, (a & 15) < (b & 15) + carry);
        self.set_flag(OF, (a ^ b) & (a ^ r) & sign != 0);
        self.szp(r, wide);
        r
    }
    fn logic(&mut self, v: u16, wide: bool) -> u16 {
        self.set_flag(CF | OF | AF, false);
        self.szp(v, wide);
        v
    }
    fn alu(&mut self, kind: usize, a: u16, b: u16, wide: bool) -> u16 {
        let carry = u16::from(self.flag(CF));
        match kind {
            0 => self.add(a, b, 0, wide),
            1 => self.logic(a | b, wide),
            2 => self.add(a, b, carry, wide),
            3 => self.sub(a, b, carry, wide),
            4 => self.logic(a & b, wide),
            5 | 7 => self.sub(a, b, 0, wide),
            _ => self.logic(a ^ b, wide),
        }
    }
    fn inc(&mut self, v: u16, decrement: bool, wide: bool) -> u16 {
        let cf = self.flag(CF);
        let r = if decrement {
            self.sub(v, 1, 0, wide)
        } else {
            self.add(v, 1, 0, wide)
        };
        self.set_flag(CF, cf);
        r
    }
    fn condition(&self, c: u8) -> bool {
        match c {
            0 => self.flag(OF),
            1 => !self.flag(OF),
            2 => self.flag(CF),
            3 => !self.flag(CF),
            4 => self.flag(ZF),
            5 => !self.flag(ZF),
            6 => self.flag(CF | ZF),
            7 => !self.flag(CF | ZF),
            8 => self.flag(SF),
            9 => !self.flag(SF),
            10 => self.flag(PF),
            11 => !self.flag(PF),
            12 => self.flag(SF) != self.flag(OF),
            13 => self.flag(SF) == self.flag(OF),
            14 => self.flag(ZF) || self.flag(SF) != self.flag(OF),
            _ => !self.flag(ZF) && self.flag(SF) == self.flag(OF),
        }
    }
    fn shift(&mut self, kind: usize, v: u16, count: u8, wide: bool) -> u16 {
        let count = if self.model186 { count & 31 } else { count };
        if count == 0 {
            return v;
        }
        let mask = if wide { 65535 } else { 255 };
        let sign = if wide { 0x8000 } else { 0x80 };
        let mut x = v & mask;
        let kind = if kind == 6 && self.model186 { 4 } else { kind };
        if kind == 6 {
            self.set_flag(CF | OF | AF, false);
            self.szp(mask, wide);
            return mask;
        }
        for _ in 0..count {
            let old = x;
            let carry = self.flag(CF);
            let last = if kind == 0 || kind == 2 || kind == 4 {
                old & sign != 0
            } else {
                old & 1 != 0
            };
            x = match kind {
                0 => ((old << 1) & mask) | u16::from(last),
                1 => (old >> 1) | if last { sign } else { 0 },
                2 => ((old << 1) & mask) | u16::from(carry),
                3 => (old >> 1) | if carry { sign } else { 0 },
                4 => (old << 1) & mask,
                5 => old >> 1,
                _ => (old >> 1) | (old & sign),
            };
            self.set_flag(CF, last);
            self.set_flag(
                OF,
                match kind {
                    0 | 2 | 4 => (x & sign != 0) != last,
                    1 | 3 => (x & sign != 0) != (x & (sign >> 1) != 0),
                    5 => old & sign != 0,
                    _ => false,
                },
            );
        }
        if kind >= 4 {
            self.szp(x, wide);
            self.set_flag(AF, false)
        }
        x
    }
    fn string<B: Bus>(&mut self, bus: &mut B, op: u8) {
        let wide = op & 1 != 0;
        let step = if wide { 2u16 } else { 1 };
        let delta = if self.flag(DF) {
            step.wrapping_neg()
        } else {
            step
        };
        let check = matches!(op, 0xa6 | 0xa7 | 0xae | 0xaf);
        let repeated = self.rep != 0;
        while !repeated || self.r[CX] != 0 {
            self.cycles += 5;
            let src = Operand {
                reg: None,
                seg: self.data(DS),
                off: self.r[SI],
            };
            let dst = Operand {
                reg: None,
                seg: self.s[ES],
                off: self.r[DI],
            };
            match op {
                0xa4 | 0xa5 => {
                    let v = self.get(bus, src, wide);
                    self.put(bus, dst, wide, v)
                }
                0xa6 | 0xa7 => {
                    let a = self.get(bus, src, wide);
                    let b = self.get(bus, dst, wide);
                    self.sub(a, b, 0, wide);
                }
                0xaa | 0xab => {
                    let v = if wide {
                        self.r[AX]
                    } else {
                        u16::from(self.reg8(0))
                    };
                    self.put(bus, dst, wide, v)
                }
                0xac | 0xad => {
                    let v = self.get(bus, src, wide);
                    self.put(bus, Operand::register(AX), wide, v)
                }
                _ => {
                    let a = if wide {
                        self.r[AX]
                    } else {
                        u16::from(self.reg8(0))
                    };
                    let b = self.get(bus, dst, wide);
                    self.sub(a, b, 0, wide);
                }
            }
            if matches!(op, 0xa4..=0xa7 | 0xac | 0xad) {
                self.r[SI] = self.r[SI].wrapping_add(delta)
            }
            if !matches!(op, 0xac | 0xad) {
                self.r[DI] = self.r[DI].wrapping_add(delta)
            }
            if !repeated {
                break;
            }
            self.r[CX] = self.r[CX].wrapping_sub(1);
            if check && self.flag(ZF) != (self.rep == 0xf3) {
                break;
            }
        }
    }
    fn bcd(&mut self, op: u8) {
        let al = self.reg8(0);
        let af = self.flag(AF);
        let cf = self.flag(CF);
        let low = (al & 15) > 9 || af;
        if op == 0x37 || op == 0x3f {
            if low {
                self.set8(
                    0,
                    if op == 0x37 {
                        al.wrapping_add(6)
                    } else {
                        al.wrapping_sub(6)
                    },
                );
                let ah = self.reg8(4);
                self.set8(
                    4,
                    if op == 0x37 {
                        ah.wrapping_add(1)
                    } else {
                        ah.wrapping_sub(1)
                    },
                );
            }
            self.set_flag(AF | CF, low);
            self.set8(0, self.reg8(0) & 15);
        } else {
            let high = cf || al > 0x9f || (al > 0x99 && !af);
            let adjust = if low { 6 } else { 0 } + if high { 0x60 } else { 0 };
            let r = if op == 0x27 {
                al.wrapping_add(adjust)
            } else {
                al.wrapping_sub(adjust)
            };
            self.set8(0, r);
            self.set_flag(AF, low);
            self.set_flag(CF, high);
            self.szp(u16::from(r), false);
        }
    }
    fn fault(&self, op: u8, reason: &str) -> String {
        format!(
            "native CPU {:04x}:{:04x} opcode {op:02x}: {reason}",
            self.start.0, self.start.1
        )
    }
}
