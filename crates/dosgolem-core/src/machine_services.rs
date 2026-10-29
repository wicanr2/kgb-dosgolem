// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
// DOS/BIOS subset ported from RRSAL Go dosgolem internal/dos at f79eab6.
// Unknown services stop this research core. No game-specific addresses here.
use super::*;
impl Machine {
    fn teletype(&mut self, ch: u8) {
        if self.bus.vga.mode != 3 {
            return;
        }
        let page = usize::from(self.bus.ram[0x462] & 7);
        let cursor = self.bus.word(0x450 + page * 2);
        let mut row = usize::from(cursor >> 8).min(24);
        let mut col = usize::from(cursor & 255).min(79);
        let base = 0xb8000 + page * 0x1000;
        match ch {
            13 => col = 0,
            10 => row += 1,
            8 => col = col.saturating_sub(1),
            7 => {}
            _ => {
                let at = base + (row * 80 + col) * 2;
                self.bus.ram[at] = ch;
                if self.bus.ram[at + 1] == 0 {
                    self.bus.ram[at + 1] = 7
                }
                col += 1;
                if col == 80 {
                    col = 0;
                    row += 1
                }
            }
        }
        if row >= 25 {
            self.bus.ram.copy_within(base + 160..base + 4000, base);
            for i in 0..80 {
                self.bus.set_word(base + 3840 + i * 2, 0x0720)
            }
            row = 24
        }
        self.bus
            .set_word(0x450 + page * 2, ((row as u16) << 8) | col as u16);
    }
    pub(super) fn dos(&mut self, ah: u8, al: u8) -> Result<(), String> {
        let ds = self.cpu.s[DS];
        let dx = self.cpu.r[DX];
        let bx = self.cpu.r[BX];
        let cx = self.cpu.r[CX];
        let es = self.cpu.s[ES];
        match ah {
            0 | 0x4c => self.terminate(),
            1 | 7 | 8 => {
                if let Some(v) = self.pop_key() {
                    self.cpu.set8(0, v as u8);
                    self.blocked = false;
                    if ah == 1 {
                        self.console.push(v as u8)
                    }
                } else {
                    self.blocked = true;
                    self.cpu.s[CS] = self.cpu.start.0;
                    self.cpu.ip = self.cpu.start.1
                }
            }
            2 => {
                self.console.push(dx as u8);
                self.teletype(dx as u8);
                self.cpu.set8(0, dx as u8)
            }
            6 => {
                if dx as u8 == 255 {
                    let v = self.pop_key();
                    self.cpu.set_flag(ZF, v.is_none());
                    self.cpu.set8(0, v.unwrap_or(0) as u8)
                } else {
                    self.console.push(dx as u8);
                    self.teletype(dx as u8);
                }
            }
            9 => {
                for i in 0..65536 {
                    let v = self.bus.ram[Cpu::address(ds, dx.wrapping_add(i as u16))];
                    if v == b'$' {
                        break;
                    }
                    self.console.push(v);
                    self.teletype(v)
                }
            }
            0x0b => self
                .cpu
                .set8(0, if self.peek_key().is_some() { 255 } else { 0 }),
            0x0c => {
                self.bus.set_word(0x41a, self.bus.word(0x41c));
                self.keys.clear();
                if matches!(al, 1 | 6 | 7 | 8) {
                    return self.dos(al, 0);
                }
                return Err(format!("unsupported flush/input {al:02x}"));
            }
            0x0d => {
                for f in self.handles.values() {
                    f.sync_all().map_err(|e| e.to_string())?
                }
            }
            0x0e => self.cpu.set8(0, 3),
            0x19 => self.cpu.set8(0, 2),
            0x1a => self.dta = (ds, dx),
            0x25 => {
                self.bus.set_word(usize::from(al) * 4, dx);
                self.bus.set_word(usize::from(al) * 4 + 2, ds)
            }
            0x2a => {
                self.cpu.r[CX] = 2026;
                self.cpu.r[DX] = 0x0a1c;
                self.cpu.set8(0, 3)
            }
            0x2c => {
                let hundredths = self.cpu.cycles / 47727;
                let seconds = hundredths / 100;
                self.cpu.r[CX] = ((seconds / 3600 % 24) as u16) << 8 | (seconds / 60 % 60) as u16;
                self.cpu.r[DX] = ((seconds % 60) as u16) << 8 | (hundredths % 100) as u16
            }
            0x2f => {
                self.cpu.s[ES] = self.dta.0;
                self.cpu.r[BX] = self.dta.1
            }
            0x30 => self.cpu.r[AX] = 5,
            0x33 => {
                if al > 1 {
                    return Err(format!("unsupported Ctrl-Break service {al:02x}"));
                }
                self.cpu.r[DX] = 0
            }
            0x35 => {
                self.cpu.r[BX] = self.bus.word(usize::from(al) * 4);
                self.cpu.s[ES] = self.bus.word(usize::from(al) * 4 + 2)
            }
            0x36 => {
                self.cpu.r[AX] = 8;
                self.cpu.r[BX] = 20000;
                self.cpu.r[CX] = 512;
                self.cpu.r[DX] = 40000
            }
            0x3c | 0x3d => {
                let name = self.cstring(ds, dx)?;
                let path = self.resolve(&name, ah == 0x3c);
                if let Ok(path) = path {
                    let mut options = OpenOptions::new();
                    options.read(true);
                    if ah == 0x3c {
                        options.write(true).create(true).truncate(true);
                    } else if al & 3 != 0 {
                        options.write(true);
                    }
                    match options.open(path) {
                        Ok(file) => {
                            let h = (5..0xffff)
                                .find(|h| !self.handles.contains_key(h))
                                .ok_or("DOS handle table full")?;
                            self.handles.insert(h, file);
                            self.cpu.r[AX] = h
                        }
                        Err(_) => {
                            self.fail(5);
                            return Ok(());
                        }
                    }
                } else {
                    self.fail(2);
                    return Ok(());
                }
            }
            0x3e => {
                if bx >= 5 && self.handles.remove(&bx).is_none() {
                    self.fail(6);
                    return Ok(());
                }
            }
            0x3f => {
                let mut bytes = vec![0; usize::from(cx)];
                let count = if bx == 0 {
                    if let Some(v) = self.pop_key() {
                        if !bytes.is_empty() {
                            bytes[0] = v as u8;
                            1
                        } else {
                            0
                        }
                    } else {
                        self.blocked = true;
                        self.cpu.s[CS] = self.cpu.start.0;
                        self.cpu.ip = self.cpu.start.1;
                        return Ok(());
                    }
                } else if let Some(file) = self.handles.get_mut(&bx) {
                    file.read(&mut bytes).map_err(|e| e.to_string())?
                } else {
                    self.fail(6);
                    return Ok(());
                };
                for (i, &v) in bytes[..count].iter().enumerate() {
                    self.bus.ram[Cpu::address(ds, dx.wrapping_add(i as u16))] = v
                }
                self.cpu.r[AX] = count as u16;
            }
            0x40 => {
                let bytes: Vec<u8> = (0..cx)
                    .map(|i| self.bus.ram[Cpu::address(ds, dx.wrapping_add(i))])
                    .collect();
                if bx <= 2 {
                    self.console.extend_from_slice(&bytes);
                    for &b in &bytes {
                        self.teletype(b)
                    }
                } else if let Some(f) = self.handles.get_mut(&bx) {
                    if cx == 0 {
                        let pos = f.stream_position().map_err(|e| e.to_string())?;
                        f.set_len(pos).map_err(|e| e.to_string())?
                    } else {
                        f.write_all(&bytes).map_err(|e| e.to_string())?
                    }
                } else {
                    self.fail(6);
                    return Ok(());
                }
                self.cpu.r[AX] = cx;
            }
            0x41 => {
                let name = self.cstring(ds, dx)?;
                let result = self
                    .resolve(&name, false)
                    .and_then(|p| fs::remove_file(p).map_err(|e| e.to_string()));
                if result.is_err() {
                    self.fail(2);
                    return Ok(());
                }
            }
            0x42 => {
                let offset = (u32::from(cx) << 16) | u32::from(dx);
                let seek = match al {
                    0 => SeekFrom::Start(u64::from(offset)),
                    1 => SeekFrom::Current(i64::from(offset as i32)),
                    2 => SeekFrom::End(i64::from(offset as i32)),
                    _ => {
                        self.fail(1);
                        return Ok(());
                    }
                };
                if let Some(f) = self.handles.get_mut(&bx) {
                    let pos = f.seek(seek).map_err(|e| e.to_string())?;
                    self.cpu.r[AX] = pos as u16;
                    self.cpu.r[DX] = (pos >> 16) as u16
                } else {
                    self.fail(6);
                    return Ok(());
                }
            }
            0x43 => {
                if al != 0 {
                    return Err("setting DOS file attributes not implemented".into());
                }
                let name = self.cstring(ds, dx)?;
                if let Ok(path) = self.resolve(&name, false) {
                    self.cpu.r[CX] = if path.is_dir() { 0x10 } else { 0x20 }
                } else {
                    self.fail(2);
                    return Ok(());
                }
            }
            0x44 => match al {
                0 => {
                    self.cpu.r[DX] = if bx < 5 { 0x80d3 } else { 2 };
                    self.cpu.r[AX] = 0
                }
                8 => self.cpu.r[AX] = 1,
                0x0e => self.cpu.set8(0, 0),
                _ => return Err(format!("unsupported IOCTL {al:02x}")),
            },
            0x47 => {
                self.bus.ram[Cpu::address(ds, self.cpu.r[SI])] = 0;
                self.cpu.r[AX] = 0x100
            }
            0x48 => {
                if let Some(seg) = self.allocate(bx) {
                    self.cpu.r[AX] = seg
                } else {
                    self.cpu.r[BX] = self
                        .blocks
                        .values()
                        .filter(|b| b.owner == 0)
                        .map(|b| b.size)
                        .max()
                        .unwrap_or(0);
                    self.fail(8);
                    return Ok(());
                }
            }
            0x49 => {
                if let Some(block) = es.checked_sub(1).and_then(|s| self.blocks.get_mut(&s)) {
                    block.owner = 0;
                    self.merge_free()
                } else {
                    self.fail(9);
                    return Ok(());
                }
            }
            0x4a => {
                // Xiake project spec 001 and its Go regression: a damaged MCB
                // above conventional memory reports error 7 and preserves BX.
                if es >= 0xa000 {
                    let signature = self.bus.ram[Cpu::address(es - 1, 0)];
                    if signature != b'M' && signature != b'Z' {
                        self.fail(7);
                        return Ok(());
                    }
                }
                if es == 0 || !self.resize(es - 1, bx) {
                    self.cpu.r[BX] = self
                        .blocks
                        .get(&es.wrapping_sub(1))
                        .map(|b| b.size)
                        .unwrap_or(0);
                    self.fail(8);
                    return Ok(());
                }
            }
            0x4b => {
                let name = self.cstring(ds, dx)?;
                let Ok(path) = self.resolve(&name, false) else {
                    self.fail(2);
                    return Ok(());
                };
                let raw = fs::read(path).map_err(|e| e.to_string())?;
                if al == 3 {
                    let seg = Cpu::read16(&mut self.bus, es, bx);
                    let factor = Cpu::read16(&mut self.bus, es, bx.wrapping_add(2));
                    let image = mz::decode(&raw, factor)?;
                    let at = usize::from(seg) * 16;
                    if at + image.bytes.len() > 0xa0000 {
                        return Err("overlay exceeds conventional RAM".into());
                    }
                    self.bus.ram[at..at + image.bytes.len()].copy_from_slice(&image.bytes)
                } else if al == 0 {
                    let image = mz::decode(&raw, 0)?;
                    let min =
                        (image.bytes.len().div_ceil(16) + 16 + usize::from(image.min_extra)) as u32;
                    let max =
                        (image.bytes.len().div_ceil(16) + 16 + usize::from(image.max_extra)) as u32;
                    let available = self
                        .blocks
                        .values()
                        .filter(|b| b.owner == 0)
                        .map(|b| b.size)
                        .max()
                        .unwrap_or(0);
                    if min > u32::from(available) {
                        self.fail(8);
                        return Ok(());
                    }
                    let need = u32::from(available).min(max) as u16;
                    let psp = self.allocate(need).ok_or("DOS allocation invariant")?;
                    self.blocks.get_mut(&(psp - 1)).unwrap().owner = psp;
                    self.sync_blocks();
                    let mut vectors = [0; 6];
                    for (i, n) in [0x22, 0x23, 0x24].iter().enumerate() {
                        vectors[i * 2] = self.bus.word(n * 4);
                        vectors[i * 2 + 1] = self.bus.word(n * 4 + 2)
                    }
                    let tail = (
                        Cpu::read16(&mut self.bus, es, bx.wrapping_add(4)),
                        Cpu::read16(&mut self.bus, es, bx.wrapping_add(2)),
                    );
                    let tail_start = Cpu::address(tail.0, tail.1);
                    let count = usize::from(self.bus.ram[tail_start]).min(126);
                    let args: Vec<u8> = (0..count)
                        .map(|i| {
                            self.bus.ram[Cpu::address(tail.0, tail.1.wrapping_add(i as u16 + 1))]
                        })
                        .collect();
                    self.parents.push(Parent {
                        cpu: self.cpu.clone(),
                        psp: self.psp,
                        name: self.program.clone(),
                        vectors,
                    });
                    self.load_at(&name, psp)?;
                    let at = usize::from(psp) * 16;
                    self.bus.ram[at + 0x80] = args.len() as u8;
                    self.bus.ram[at + 0x81..at + 0x81 + args.len()].copy_from_slice(&args);
                    self.bus.ram[at + 0x81 + args.len()] = 13;
                } else {
                    return Err(format!("unsupported EXEC mode {al:02x}"));
                }
            }
            0x4d => {
                self.cpu.r[AX] = self.last_exit;
                self.last_exit = 0
            }
            0x4e => {
                let name = self.cstring(ds, dx)?;
                let pattern = name.to_ascii_uppercase();
                let mut found = Vec::new();
                for e in fs::read_dir(&self.root).map_err(|e| e.to_string())? {
                    let p = e.map_err(|e| e.to_string())?.path();
                    if p.is_file()
                        && wildcard(
                            pattern.as_bytes(),
                            p.file_name()
                                .unwrap()
                                .to_string_lossy()
                                .to_ascii_uppercase()
                                .as_bytes(),
                        )
                    {
                        found.push(p)
                    }
                }
                found.sort();
                self.search = found.into();
                if !self.find_next()? {
                    self.fail(18);
                    return Ok(());
                }
            }
            0x4f => {
                if !self.find_next()? {
                    self.fail(18);
                    return Ok(());
                }
            }
            0x51 | 0x62 => self.cpu.r[BX] = self.psp,
            0x52 => {
                self.cpu.s[ES] = 0x100;
                self.cpu.r[BX] = 0x10;
                self.bus.set_word(0x100e, 0xcf)
            }
            0x57 if al == 0 => {
                self.cpu.r[CX] = 0;
                self.cpu.r[DX] = 0x0021
            }
            0x59 => {
                self.cpu.r[AX] = self.last_error;
                self.cpu.r[BX] = 0x0101;
                self.cpu.r[CX] = 0x0100
            }
            0x67 => {}
            _ => {
                return Err(format!(
                    "unsupported DOS INT 21 AH={ah:02x} AL={al:02x} at {:04x}:{:04x}",
                    self.cpu.start.0, self.cpu.start.1
                ));
            }
        }
        self.cpu.set_flag(CF, false);
        Ok(())
    }
    fn find_next(&mut self) -> Result<bool, String> {
        let Some(path) = self.search.pop_front() else {
            return Ok(false);
        };
        let meta = path.metadata().map_err(|e| e.to_string())?;
        let at = Cpu::address(self.dta.0, self.dta.1);
        if at + 43 >= 0xa0000 {
            return Err("DTA exceeds conventional memory".into());
        }
        self.bus.ram[at..at + 43].fill(0);
        self.bus.ram[at + 21] = 0x20;
        self.bus.set_word(at + 24, 0x21);
        self.bus.set_word(at + 26, meta.len() as u16);
        self.bus.set_word(at + 28, (meta.len() >> 16) as u16);
        let name = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_uppercase();
        let name = name.as_bytes();
        let n = name.len().min(12);
        self.bus.ram[at + 30..at + 30 + n].copy_from_slice(&name[..n]);
        Ok(true)
    }
    pub(super) fn video(&mut self, ah: u8, al: u8) -> Result<(), String> {
        match ah {
            0 => {
                if !matches!(al & 0x7f, 3 | 0x0d..=0x12) {
                    return Err(format!("unsupported video mode {al:02x}"));
                }
                self.bus.vga.reset(al & 0x7f);
                self.bus.ram[0x449] = al & 0x7f;
                self.bus.set_word(0x44a, 80);
                self.bus
                    .set_word(0x44c, if al & 0x7f == 0x10 { 0x8000 } else { 0x1000 });
                self.bus.set_word(0x44e, 0);
                self.bus.ram[0x462] = 0;
            }
            1 => self.bus.set_word(0x460, self.cpu.r[CX]),
            2 => {
                let page = usize::from(self.cpu.reg8(7) & 7);
                self.bus.set_word(0x450 + page * 2, self.cpu.r[DX])
            }
            3 => {
                let page = usize::from(self.cpu.reg8(7) & 7);
                self.cpu.r[DX] = self.bus.word(0x450 + page * 2);
                self.cpu.r[CX] = self.bus.word(0x460)
            }
            5 => {
                let size = if self.bus.vga.mode == 0x10 {
                    0x8000usize
                } else {
                    usize::from(self.bus.word(0x44c))
                };
                let at = size * usize::from(al);
                if at > 65535 {
                    return Err("video page out of bounds".into());
                }
                self.bus.ram[0x462] = al;
                self.bus.set_word(0x44e, at as u16);
                self.bus.vga.crtc[12] = (at >> 8) as u8;
                self.bus.vga.crtc[13] = at as u8
            }
            6 | 7 => {
                if self.bus.vga.mode != 3 {
                    return Err("graphics text scrolling not implemented".into());
                }
                let top = usize::from(self.cpu.reg8(5));
                let left = usize::from(self.cpu.reg8(1));
                let bottom = usize::from(self.cpu.reg8(6)).min(24);
                let right = usize::from(self.cpu.reg8(2)).min(79);
                if top <= bottom && left <= right {
                    for row in top..=bottom {
                        for col in left..=right {
                            let at = 0xb8000 + row * 160 + col * 2;
                            self.bus.ram[at] = b' ';
                            self.bus.ram[at + 1] = self.cpu.reg8(7)
                        }
                    }
                }
                if al != 0 {
                    return Err("non-clearing text scroll not implemented".into());
                }
            }
            8 => {
                if self.bus.vga.mode != 3 {
                    return Err("reading graphics glyphs is not implemented".into());
                }
                let page = if self.cpu.reg8(7) == 255 {
                    self.bus.ram[0x462]
                } else {
                    self.cpu.reg8(7)
                };
                let cursor = self.bus.word(0x450 + usize::from(page) * 2);
                let at = 0xb8000
                    + usize::from(page) * usize::from(self.bus.word(0x44c))
                    + (usize::from(cursor >> 8) * usize::from(self.bus.word(0x44a))
                        + usize::from(cursor & 255))
                        * 2;
                self.cpu.r[AX] = self.bus.word(at);
            }
            9 | 0x0a => {
                if self.bus.vga.mode != 3 {
                    return Err("BIOS font graphics output not implemented".into());
                }
                let page = usize::from(self.cpu.reg8(7) & 7);
                let cursor = self.bus.word(0x450 + page * 2);
                let mut row = usize::from(cursor >> 8);
                let mut col = usize::from(cursor & 255);
                for _ in 0..self.cpu.r[CX] {
                    if row >= 25 {
                        break;
                    }
                    let at = 0xb8000 + page * 0x1000 + row * 160 + col * 2;
                    self.bus.ram[at] = al;
                    if ah == 9 {
                        self.bus.ram[at + 1] = self.cpu.reg8(3)
                    }
                    col += 1;
                    if col >= 80 {
                        col = 0;
                        row += 1
                    }
                }
            }
            0x0e => {
                self.console.push(al);
                self.teletype(al)
            }
            0x0f => {
                self.cpu.r[AX] = 0x5000 | u16::from(self.bus.vga.mode);
                self.cpu.set8(7, self.bus.ram[0x462])
            }
            0x10 => match al {
                0 => {
                    let i = usize::from(self.cpu.reg8(3) & 15);
                    self.bus.vga.ac[i] = self.cpu.reg8(7)
                }
                1 => self.bus.vga.ac[0x11] = self.cpu.reg8(7),
                2 => {
                    let at = Cpu::address(self.cpu.s[ES], self.cpu.r[DX]);
                    for i in 0..16 {
                        self.bus.vga.ac[i] = self.bus.ram[at + i]
                    }
                    self.bus.vga.ac[0x11] = self.bus.ram[at + 16]
                }
                7 => {
                    let i = usize::from(self.cpu.reg8(3) & 15);
                    self.cpu.set8(7, self.bus.vga.ac[i]);
                }
                8 => self.cpu.set8(7, self.bus.vga.ac[0x11]),
                9 => {
                    let at = Cpu::address(self.cpu.s[ES], self.cpu.r[DX]);
                    self.bus.ram[at..at + 16].copy_from_slice(&self.bus.vga.ac[..16]);
                    self.bus.ram[at + 16] = self.bus.vga.ac[0x11];
                }
                0x10 => {
                    let i = usize::from(self.cpu.r[BX] & 255);
                    self.bus.vga.dac[i] = [
                        self.cpu.reg8(6) & 63,
                        self.cpu.reg8(5) & 63,
                        self.cpu.reg8(1) & 63,
                    ]
                }
                0x12 => {
                    let at = Cpu::address(self.cpu.s[ES], self.cpu.r[DX]);
                    for i in 0..usize::from(self.cpu.r[CX]) {
                        let index = usize::from(self.cpu.r[BX]) + i;
                        if index >= 256 || at + i * 3 + 3 > 0xa0000 {
                            return Err("DAC block out of bounds".into());
                        }
                        self.bus.vga.dac[index]
                            .copy_from_slice(&self.bus.ram[at + i * 3..at + i * 3 + 3])
                    }
                }
                0x15 => {
                    let c = self.bus.vga.dac[usize::from(self.cpu.r[BX] & 255)];
                    self.cpu.set8(6, c[0]);
                    self.cpu.set8(5, c[1]);
                    self.cpu.set8(1, c[2]);
                }
                0x17 => {
                    let at = Cpu::address(self.cpu.s[ES], self.cpu.r[DX]);
                    for i in 0..usize::from(self.cpu.r[CX]) {
                        let index = usize::from(self.cpu.r[BX]) + i;
                        if index >= 256 || at + i * 3 + 3 > 0xa0000 {
                            return Err("DAC read block out of bounds".into());
                        }
                        self.bus.ram[at + i * 3..at + i * 3 + 3]
                            .copy_from_slice(&self.bus.vga.dac[index]);
                    }
                }
                _ => return Err(format!("unsupported BIOS palette {al:02x}")),
            },
            0x12 if self.cpu.reg8(3) == 0x10 => {
                self.cpu.r[BX] = 0x0003;
                self.cpu.r[CX] = 0x0009
            }
            0x1a => {
                self.cpu.set8(0, 0x1a);
                self.cpu.r[BX] = 8
            }
            _ => return Err(format!("unsupported BIOS video AH={ah:02x} AL={al:02x}")),
        }
        Ok(())
    }
}
fn wildcard(pattern: &[u8], text: &[u8]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }
    if pattern == b"*.*" {
        return true;
    }
    match pattern[0] {
        b'*' => (0..=text.len()).any(|n| wildcard(&pattern[1..], &text[n..])),
        b'?' => !text.is_empty() && wildcard(&pattern[1..], &text[1..]),
        c => !text.is_empty() && text[0] == c && wildcard(&pattern[1..], &text[1..]),
    }
}
