// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
// Port of RRSAL Go dosgolem internal/cpu/{ops,ops186,alu}.go at f79eab6.
// Original: Wang Chun-Yu / wicanr2, https://github.com/wicanr2/dosgolem .
use super::*;

impl Cpu {
    /// One complete instruction. Software INT is returned to the DOS owner;
    /// an unhandled INT must be passed to `interrupt`, never ignored.
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> Result<Option<u8>, String> {
        self.start = (self.s[CS], self.ip);
        self.segment = None;
        self.rep = 0;
        self.cycles += 2;
        self.steps += 1;
        for _ in 0..64 {
            let op = self.fetch(bus);
            match op {
                0x26 => self.segment = Some(ES),
                0x2e => self.segment = Some(CS),
                0x36 => self.segment = Some(SS),
                0x3e => self.segment = Some(DS),
                0xf0 | 0xf1 => {}
                0xf2 | 0xf3 => self.rep = op,
                _ => return self.execute(bus, op),
            }
            self.cycles += 1;
        }
        Err(self.fault(0, "excessive prefixes"))
    }
    fn execute<B: Bus>(&mut self, bus: &mut B, op: u8) -> Result<Option<u8>, String> {
        let wide = op & 1 != 0;
        if op < 0x40 && op & 7 < 6 {
            let kind = usize::from(op >> 3);
            let form = op & 7;
            let (dst, a, b) = if form < 4 {
                let (reg, rm) = self.decode(bus);
                let r = Operand::register(reg);
                let (dst, src) = if form < 2 { (rm, r) } else { (r, rm) };
                (dst, self.get(bus, dst, wide), self.get(bus, src, wide))
            } else {
                (
                    Operand::register(AX),
                    if wide {
                        self.r[AX]
                    } else {
                        u16::from(self.reg8(0))
                    },
                    self.immediate(bus, wide),
                )
            };
            let v = self.alu(kind, a, b, wide);
            if kind != 7 {
                self.put(bus, dst, wide, v)
            }
            return Ok(None);
        }
        match op {
            0x06 | 0x0e | 0x16 | 0x1e => self.push(bus, self.s[usize::from(op >> 3)]),
            0x07 | 0x0f | 0x17 | 0x1f => {
                let v = self.pop(bus);
                self.s[usize::from(op >> 3)] = v
            }
            0x27 | 0x2f | 0x37 | 0x3f => self.bcd(op),
            0x40..=0x4f => {
                let i = usize::from(op & 7);
                self.r[i] = self.inc(self.r[i], op >= 0x48, true)
            }
            0x50..=0x57 => self.push_operand(bus, Operand::register(usize::from(op & 7))),
            0x58..=0x5f => {
                let v = self.pop(bus);
                self.r[usize::from(op & 7)] = v
            }
            0x60..=0x6f if self.model186 => self.extended186(bus, op)?,
            0x60..=0x7f => {
                let off = self.fetch(bus) as i8 as i16 as u16;
                if self.condition(op & 15) {
                    self.cycles += 5;
                    self.ip = self.ip.wrapping_add(off)
                }
            }
            0x80..=0x83 => {
                let (kind, dst) = self.decode(bus);
                let w = op == 0x81 || op == 0x83;
                let b = if op == 0x83 {
                    self.fetch(bus) as i8 as i16 as u16
                } else {
                    self.immediate(bus, w)
                };
                let a = self.get(bus, dst, w);
                let v = self.alu(kind, a, b, w);
                if kind != 7 {
                    self.put(bus, dst, w, v)
                }
            }
            0x84 | 0x85 => {
                let (reg, rm) = self.decode(bus);
                let a = self.get(bus, rm, wide);
                let b = self.get(bus, Operand::register(reg), wide);
                self.logic(a & b, wide);
            }
            0x86 | 0x87 => {
                let (reg, rm) = self.decode(bus);
                let dst = Operand::register(reg);
                let a = self.get(bus, rm, wide);
                let b = self.get(bus, dst, wide);
                self.put(bus, rm, wide, b);
                self.put(bus, dst, wide, a)
            }
            0x88..=0x8b => {
                let (reg, rm) = self.decode(bus);
                let r = Operand::register(reg);
                let (dst, src) = if op < 0x8a { (rm, r) } else { (r, rm) };
                let v = self.get(bus, src, wide);
                self.put(bus, dst, wide, v)
            }
            0x8c => {
                let (reg, rm) = self.decode(bus);
                self.put(bus, rm, true, self.s[reg & 3])
            }
            0x8d => {
                let (reg, rm) = self.decode(bus);
                self.r[reg] = rm.off
            }
            0x8e => {
                self.cycles += 18;
                let (reg, rm) = self.decode(bus);
                self.s[reg & 3] = self.get(bus, rm, true)
            }
            0x8f => {
                let (_, rm) = self.decode(bus);
                let v = self.pop(bus);
                self.put(bus, rm, true, v)
            }
            0x90..=0x97 => self.r.swap(AX, usize::from(op & 7)),
            0x98 => self.r[AX] = self.reg8(0) as i8 as i16 as u16,
            0x99 => self.r[DX] = if self.r[AX] & 0x8000 != 0 { 0xffff } else { 0 },
            0x9a => {
                let off = self.immediate(bus, true);
                let seg = self.immediate(bus, true);
                self.push(bus, self.s[CS]);
                self.push(bus, self.ip);
                self.s[CS] = seg;
                self.ip = off
            }
            0x9b => {}
            0x9c => self.push(bus, self.flags),
            0x9d => {
                let f = self.pop(bus);
                self.set_flags(f)
            }
            0x9e => self.set_flags((self.flags & 0xff00) | u16::from(self.reg8(4))),
            0x9f => self.set8(4, self.flags as u8),
            0xa0..=0xa3 => {
                let off = self.immediate(bus, true);
                let mem = Operand {
                    reg: None,
                    seg: self.data(DS),
                    off,
                };
                let r = Operand::register(AX);
                let (dst, src) = if op < 0xa2 { (r, mem) } else { (mem, r) };
                let v = self.get(bus, src, wide);
                self.put(bus, dst, wide, v)
            }
            0xa8 | 0xa9 => {
                let b = self.immediate(bus, wide);
                let a = if wide {
                    self.r[AX]
                } else {
                    u16::from(self.reg8(0))
                };
                self.logic(a & b, wide);
            }
            0xa4..=0xaf => self.string(bus, op),
            0xb0..=0xb7 => {
                let v = self.fetch(bus);
                self.set8(usize::from(op & 7), v)
            }
            0xb8..=0xbf => {
                let v = self.immediate(bus, true);
                self.r[usize::from(op & 7)] = v
            }
            0xc0 | 0xc1 if self.model186 => {
                let (kind, rm) = self.decode(bus);
                let count = self.fetch(bus);
                let v = self.get(bus, rm, wide);
                let r = self.shift(kind, v, count, wide);
                self.put(bus, rm, wide, r)
            }
            0xc8 if self.model186 => {
                let allocation = self.immediate(bus, true);
                let level = self.fetch(bus) & 31;
                self.push(bus, self.r[BP]);
                let frame = self.r[SP];
                for _ in 1..level {
                    self.r[BP] = self.r[BP].wrapping_sub(2);
                    let v = Self::read16(bus, self.s[SS], self.r[BP]);
                    self.push(bus, v)
                }
                if level > 0 {
                    self.push(bus, frame)
                }
                self.r[BP] = frame;
                self.r[SP] = self.r[SP].wrapping_sub(allocation);
            }
            0xc9 if self.model186 => {
                self.r[SP] = self.r[BP];
                self.r[BP] = self.pop(bus)
            }
            0xc0..=0xc3 => {
                self.cycles += 8;
                let n = if op & 1 == 0 {
                    self.immediate(bus, true)
                } else {
                    0
                };
                self.ip = self.pop(bus);
                self.r[SP] = self.r[SP].wrapping_add(n)
            }
            0xc4 | 0xc5 => {
                self.cycles += 18;
                let (reg, rm) = self.decode(bus);
                self.r[reg] = self.get(bus, rm, true);
                self.s[if op == 0xc4 { ES } else { DS }] =
                    Self::read16(bus, rm.seg, rm.off.wrapping_add(2))
            }
            0xc6 | 0xc7 => {
                let (_, rm) = self.decode(bus);
                let v = self.immediate(bus, wide);
                self.put(bus, rm, wide, v)
            }
            0xc8..=0xcb => {
                self.cycles += 18;
                let n = if op & 1 == 0 {
                    self.immediate(bus, true)
                } else {
                    0
                };
                self.ip = self.pop(bus);
                self.s[CS] = self.pop(bus);
                self.r[SP] = self.r[SP].wrapping_add(n)
            }
            0xcc => return Ok(Some(3)),
            0xcd => return Ok(Some(self.fetch(bus))),
            0xce => {
                if self.flag(OF) {
                    return Ok(Some(4));
                }
            }
            0xcf => {
                self.ip = self.pop(bus);
                self.s[CS] = self.pop(bus);
                let f = self.pop(bus);
                self.set_flags(f)
            }
            0xd0..=0xd3 => {
                let (kind, rm) = self.decode(bus);
                let count = if op < 0xd2 { 1 } else { self.reg8(1) };
                let v = self.get(bus, rm, wide);
                let r = self.shift(kind, v, count, wide);
                self.put(bus, rm, wide, r)
            }
            0xd4 => {
                let divisor = self.fetch(bus);
                let al = self.reg8(0);
                if let Some(quotient) = al.checked_div(divisor) {
                    self.set8(4, quotient);
                    self.set8(0, al % divisor);
                    self.logic(u16::from(al % divisor), false);
                } else {
                    self.logic(0, false);
                    self.interrupt(bus, 0)
                }
            }
            0xd5 => {
                let base = self.fetch(bus);
                let v = self.reg8(4).wrapping_mul(base).wrapping_add(self.reg8(0));
                self.r[AX] = u16::from(v);
                self.logic(u16::from(v), false);
            }
            0xd6 => self.set8(0, if self.flag(CF) { 255 } else { 0 }),
            0xd7 => {
                let a = Self::address(
                    self.data(DS),
                    self.r[BX].wrapping_add(u16::from(self.reg8(0))),
                );
                self.set8(0, bus.read(a))
            }
            0xd8..=0xdf => {
                let (reg, rm) = self.decode(bus);
                if op == 0xdb && reg == 4 && rm.reg == Some(3) {
                    self.fpu_control = 0x37f
                } else if op == 0xd9 && reg == 7 && rm.reg.is_none() {
                    self.put(bus, rm, true, self.fpu_control)
                } else if self.model186 {
                    return Err(self.fault(op, "x87 arithmetic is not supported"));
                } else if rm.reg.is_none() {
                    self.get(bus, rm, true);
                }
            }
            0xe0..=0xe3 => {
                let off = self.fetch(bus) as i8 as i16 as u16;
                if op != 0xe3 {
                    self.r[CX] = self.r[CX].wrapping_sub(1)
                }
                let take = match op {
                    0xe0 => self.r[CX] != 0 && !self.flag(ZF),
                    0xe1 => self.r[CX] != 0 && self.flag(ZF),
                    0xe2 => self.r[CX] != 0,
                    _ => self.r[CX] == 0,
                };
                if take {
                    self.ip = self.ip.wrapping_add(off)
                }
            }
            0xe4..=0xe7 | 0xec..=0xef => {
                let port = if op < 0xec {
                    u16::from(self.fetch(bus))
                } else {
                    self.r[DX]
                };
                self.cycles += if wide { 28 } else { 14 };
                if op & 2 == 0 {
                    let lo = u16::from(bus.input(port));
                    let v = if wide {
                        lo | (u16::from(bus.input(port.wrapping_add(1))) << 8)
                    } else {
                        lo
                    };
                    self.put(bus, Operand::register(AX), wide, v)
                } else {
                    bus.output(port, self.reg8(0));
                    if wide {
                        bus.output(port.wrapping_add(1), self.reg8(4))
                    }
                }
            }
            0xe8 => {
                self.cycles += 7;
                let off = self.immediate(bus, true);
                self.push(bus, self.ip);
                self.ip = self.ip.wrapping_add(off)
            }
            0xe9 => {
                self.cycles += 5;
                let off = self.immediate(bus, true);
                self.ip = self.ip.wrapping_add(off)
            }
            0xea => {
                self.cycles += 18;
                let off = self.immediate(bus, true);
                let seg = self.immediate(bus, true);
                self.ip = off;
                self.s[CS] = seg
            }
            0xeb => {
                self.cycles += 5;
                let off = self.fetch(bus) as i8 as i16 as u16;
                self.ip = self.ip.wrapping_add(off)
            }
            0xf4 => self.halted = true,
            0xf5 => self.set_flag(CF, !self.flag(CF)),
            0xf6 | 0xf7 => self.group3(bus, op)?,
            0xf8 | 0xf9 => self.set_flag(CF, op & 1 != 0),
            0xfa | 0xfb => self.set_flag(IF, op & 1 != 0),
            0xfc | 0xfd => self.set_flag(DF, op & 1 != 0),
            0xfe | 0xff => {
                let (kind, rm) = self.decode(bus);
                if kind < 2 || op == 0xfe {
                    let a = self.get(bus, rm, wide);
                    let v = self.inc(a, kind == 1, wide);
                    self.put(bus, rm, wide, v)
                } else {
                    match kind {
                        2 => {
                            let address = self.get(bus, rm, true);
                            self.push(bus, self.ip);
                            self.ip = address
                        }
                        3 | 5 => {
                            let off = self.get(bus, rm, true);
                            let seg = Self::read16(bus, rm.seg, rm.off.wrapping_add(2));
                            if kind == 3 {
                                self.push(bus, self.s[CS]);
                                self.push(bus, self.ip)
                            }
                            self.ip = off;
                            self.s[CS] = seg
                        }
                        4 => self.ip = self.get(bus, rm, true),
                        _ => self.push_operand(bus, rm),
                    }
                }
            }
            _ => return Err(self.fault(op, "unsupported instruction")),
        }
        Ok(None)
    }
    fn push_operand<B: Bus>(&mut self, bus: &mut B, o: Operand) {
        if o.reg == Some(SP) {
            self.r[SP] = self.r[SP].wrapping_sub(2);
            Self::write16(bus, self.s[SS], self.r[SP], self.r[SP]);
        } else {
            let v = self.get(bus, o, true);
            self.push(bus, v)
        }
    }
    fn extended186<B: Bus>(&mut self, bus: &mut B, op: u8) -> Result<(), String> {
        match op {
            0x60 => {
                let sp = self.r[SP];
                for i in [AX, CX, DX, BX] {
                    self.push(bus, self.r[i])
                }
                self.push(bus, sp);
                for i in [BP, SI, DI] {
                    self.push(bus, self.r[i])
                }
            }
            0x61 => {
                for i in [DI, SI, BP] {
                    self.r[i] = self.pop(bus)
                }
                self.pop(bus);
                for i in [BX, DX, CX, AX] {
                    self.r[i] = self.pop(bus)
                }
            }
            0x62 => {
                let (reg, rm) = self.decode(bus);
                if rm.reg.is_some() {
                    return Err(self.fault(op, "BOUND requires memory"));
                }
                let lo = Self::read16(bus, rm.seg, rm.off) as i16;
                let hi = Self::read16(bus, rm.seg, rm.off.wrapping_add(2)) as i16;
                let x = self.r[reg] as i16;
                if x < lo || x > hi {
                    self.s[CS] = self.start.0;
                    self.ip = self.start.1;
                    self.interrupt(bus, 5)
                }
            }
            0x68 => {
                let v = self.immediate(bus, true);
                self.push(bus, v)
            }
            0x6a => {
                let v = self.fetch(bus) as i8 as i16 as u16;
                self.push(bus, v)
            }
            0x69 | 0x6b => {
                let (reg, rm) = self.decode(bus);
                let a = i32::from(self.get(bus, rm, true) as i16);
                let b = if op == 0x69 {
                    i32::from(self.immediate(bus, true) as i16)
                } else {
                    i32::from(self.fetch(bus) as i8)
                };
                let result = a * b;
                self.r[reg] = result as u16;
                self.set_flag(CF | OF, i32::from(result as i16) != result)
            }
            0x6c..=0x6f => {
                let wide = op & 1 != 0;
                let delta = if self.flag(DF) {
                    if wide { 0xfffe } else { 0xffff }
                } else if wide {
                    2
                } else {
                    1
                };
                loop {
                    if self.rep != 0 && self.r[CX] == 0 {
                        break;
                    }
                    if op < 0x6e {
                        let lo = u16::from(bus.input(self.r[DX]));
                        let v = if wide {
                            lo | (u16::from(bus.input(self.r[DX])) << 8)
                        } else {
                            lo
                        };
                        self.put(
                            bus,
                            Operand {
                                reg: None,
                                seg: self.s[ES],
                                off: self.r[DI],
                            },
                            wide,
                            v,
                        );
                        self.r[DI] = self.r[DI].wrapping_add(delta)
                    } else {
                        let v = self.get(
                            bus,
                            Operand {
                                reg: None,
                                seg: self.data(DS),
                                off: self.r[SI],
                            },
                            wide,
                        );
                        bus.output(self.r[DX], v as u8);
                        if wide {
                            bus.output(self.r[DX], (v >> 8) as u8)
                        }
                        self.r[SI] = self.r[SI].wrapping_add(delta)
                    }
                    if self.rep == 0 {
                        break;
                    }
                    self.r[CX] = self.r[CX].wrapping_sub(1)
                }
            }
            _ => return Err(self.fault(op, "unsupported 80186 instruction")),
        }
        Ok(())
    }
    fn group3<B: Bus>(&mut self, bus: &mut B, op: u8) -> Result<(), String> {
        let (kind, rm) = self.decode(bus);
        let wide = op & 1 != 0;
        let a = self.get(bus, rm, wide);
        match kind {
            0 | 1 => {
                let b = self.immediate(bus, wide);
                self.logic(a & b, wide);
            }
            2 => self.put(bus, rm, wide, !a),
            3 => {
                let r = self.sub(0, a, 0, wide);
                self.put(bus, rm, wide, r)
            }
            4 | 5 => {
                self.cycles += if kind == 4 { 14 } else { 0 };
                let signed = kind == 5;
                if wide {
                    let full = if signed {
                        (i32::from(self.r[AX] as i16) * i32::from(a as i16)) as u32
                    } else {
                        u32::from(self.r[AX]) * u32::from(a)
                    };
                    self.r[AX] = full as u16;
                    self.r[DX] = (full >> 16) as u16;
                    self.szp(self.r[DX], true);
                    if !signed {
                        self.set_flag(ZF, full == 0)
                    }
                    let over = if signed {
                        full as i32 != i32::from(full as i16)
                    } else {
                        self.r[DX] != 0
                    };
                    self.set_flag(CF | OF, over)
                } else {
                    let full = if signed {
                        (i16::from(self.reg8(0) as i8) * i16::from(a as u8 as i8)) as u16
                    } else {
                        u16::from(self.reg8(0)) * a
                    };
                    self.r[AX] = full;
                    self.szp(full, true);
                    let over = if signed {
                        full as i16 != i16::from(full as i8)
                    } else {
                        full > 255
                    };
                    self.set_flag(CF | OF, over)
                }
            }
            6 | 7 => {
                let signed = kind == 7;
                self.cycles += if signed { 0 } else { 25 };
                let high = if wide {
                    self.r[DX]
                } else {
                    u16::from(self.reg8(4))
                };
                self.sub(high, a, 0, wide);
                let dividend = if wide {
                    (u32::from(self.r[DX]) << 16) | u32::from(self.r[AX])
                } else {
                    u32::from(self.r[AX])
                };
                let result = if a == 0 {
                    None
                } else if signed {
                    let n = if wide {
                        i64::from(dividend as i32)
                    } else {
                        i64::from(dividend as i16)
                    };
                    let d = if wide {
                        i64::from(a as i16)
                    } else {
                        i64::from(a as i8)
                    };
                    let q = n / d;
                    let bound = if wide { 32768 } else { 128 };
                    if q < -bound || q >= bound {
                        None
                    } else {
                        Some((q as u16, (n % d) as u16))
                    }
                } else {
                    let q = dividend / u32::from(a);
                    if q > if wide { 65535 } else { 255 } {
                        None
                    } else {
                        Some((q as u16, (dividend % u32::from(a)) as u16))
                    }
                };
                if let Some((q, r)) = result {
                    if wide {
                        self.r[AX] = q;
                        self.r[DX] = r
                    } else {
                        self.r[AX] = (q & 255) | ((r & 255) << 8)
                    }
                } else {
                    self.interrupt(bus, 0)
                }
            }
            _ => return Err(self.fault(op, "invalid group 3")),
        }
        Ok(())
    }
}
