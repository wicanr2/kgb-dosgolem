// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! EMS（LIM/EMS 4.0，`int 67h`）。移植自 Go dosgolem f79eab6 `internal/dos/ems.go`，
//! 契約見該庫 docs/spec/008-ems-paging、014-ems、429-ems-move-exchange-region，
//! 本庫規格 docs/spec/002-ems.md。
//!
//! 偵測：`int 67h` 向量指向 `SEG:0000`，那裡是 `int F6h; iret` trampoline，
//! `+0Ah` 是簽章 `EMMXXXX0`。page frame 在 D000h 段，四個 16 KB 實體頁。
//! 邏輯頁存在客體 1 MB 之外，映射時複製進頁框、換映射或釋放前寫回（複製式分頁）。
//! 回傳慣例：AH＝0 成功、非 0 為錯誤碼，不用 CF。
use crate::cpu::{BX, CX, Cpu, DI, DS, DX, ES, SI};
use crate::memory::Memory;
use std::collections::BTreeMap;

/// EMM driver header 所在段（Go `machine.EMSSeg`）。
pub const HEADER_SEG: u16 = 0x0050;
/// page frame 段（Go `machine.EMSFrameSeg`）。
pub const FRAME_SEG: u16 = 0xd000;
/// trampoline 用的軟體中斷號（Go dos.go `case 0xF6`）。
pub const TRAMPOLINE: u8 = 0xf6;
const PAGE: usize = 16 * 1024;
const PHYS: usize = 4;
/// 宣告的總頁數，512 × 16 KB ＝ 8 MB（Go `emsTotalPages`）。
const TOTAL: usize = 512;

#[derive(Clone, Copy, PartialEq, Debug)]
struct Mapping {
    handle: u16,
    page: u16,
}

/// 一筆 EMS 操作紀錄，給探針與對拍用。
#[derive(Clone, Debug, PartialEq)]
pub struct EmsOp {
    pub step: u64,
    pub ah: u8,
    pub al: u8,
    pub handle: u16,
    pub value: u16,
    pub status: u8,
}

#[derive(Default)]
pub struct Ems {
    handles: BTreeMap<u16, Vec<Vec<u8>>>,
    next: u16,
    frame: [Option<Mapping>; PHYS],
    saved: [Option<Mapping>; PHYS],
    pub ops: Vec<EmsOp>,
    /// 未實作的子功能（AH, AL），回 84h 或 8Fh 並記在這裡。
    pub unsupported: Vec<(u8, u8)>,
}

fn frame_addr(p: usize) -> usize {
    usize::from(FRAME_SEG) * 16 + p * PAGE
}

/// 在 1 MB 空間寫 driver header：`CD F6 CF` 與 `EMMXXXX0`，並把 `int 67h` 指過來。
pub fn install(bus: &mut Memory) {
    let base = usize::from(HEADER_SEG) * 16;
    bus.ram[base..base + 3].copy_from_slice(&[0xcd, TRAMPOLINE, 0xcf]);
    bus.ram[base + 0x0a..base + 0x12].copy_from_slice(b"EMMXXXX0");
    bus.set_word(0x67 * 4, 0);
    bus.set_word(0x67 * 4 + 2, HEADER_SEG);
}

impl Ems {
    pub fn new() -> Self {
        Self {
            next: 1,
            ..Self::default()
        }
    }
    fn used(&self) -> usize {
        self.handles.values().map(Vec::len).sum()
    }
    fn flush(&mut self, bus: &Memory, p: usize) {
        let Some(m) = self.frame[p] else { return };
        if let Some(pages) = self.handles.get_mut(&m.handle) {
            if let Some(page) = pages.get_mut(usize::from(m.page)) {
                let base = frame_addr(p);
                page.copy_from_slice(&bus.ram[base..base + PAGE]);
            }
        }
    }
    fn load(&mut self, bus: &mut Memory, p: usize, handle: u16, page: u16) {
        let base = frame_addr(p);
        bus.ram[base..base + PAGE].copy_from_slice(&self.handles[&handle][usize::from(page)]);
        self.frame[p] = Some(Mapping { handle, page });
    }
    /// 所有映射中的實體頁寫回邏輯頁（傾印或搜尋前呼叫）。
    pub fn flush_all(&mut self, bus: &Memory) {
        for p in 0..PHYS {
            self.flush(bus, p)
        }
    }
    /// 各 handle 的頁數。
    pub fn layout(&self) -> Vec<(u16, usize)> {
        self.handles.iter().map(|(&h, p)| (h, p.len())).collect()
    }

    /// `int 67h` 分派。
    pub fn call(&mut self, cpu: &mut Cpu, bus: &mut Memory) {
        let (ah, al) = (cpu.reg8(4), cpu.reg8(0));
        let status = match ah {
            0x40 => 0,
            0x41 => {
                cpu.r[BX] = FRAME_SEG;
                0
            }
            0x42 => {
                cpu.r[BX] = (TOTAL - self.used()) as u16;
                cpu.r[DX] = TOTAL as u16;
                0
            }
            0x43 => self.alloc(cpu),
            0x44 => self.map(cpu, bus),
            0x45 => self.release(cpu, bus),
            0x46 => {
                cpu.set8(0, 0x40);
                0
            }
            0x47 => {
                self.saved = self.frame;
                0
            }
            0x48 => {
                self.flush_all(bus);
                for p in 0..PHYS {
                    match self.saved[p] {
                        None => self.frame[p] = None,
                        Some(m) => {
                            if self
                                .handles
                                .get(&m.handle)
                                .is_some_and(|h| usize::from(m.page) < h.len())
                            {
                                self.load(bus, p, m.handle, m.page)
                            }
                        }
                    }
                }
                0
            }
            0x4b => {
                cpu.r[BX] = self.handles.len() as u16;
                0
            }
            0x4c => match self.handles.get(&cpu.r[DX]) {
                Some(h) => {
                    cpu.r[BX] = h.len() as u16;
                    0
                }
                None => 0x83,
            },
            0x4e => self.page_map(cpu, bus, al),
            0x50 => self.map_multiple(cpu, bus, al),
            0x51 => self.realloc(cpu, bus),
            0x57 => self.move_region(cpu, bus, al),
            0x58 => {
                if al == 0 {
                    let dst = Cpu::address(cpu.s[ES], cpu.r[DI]);
                    for p in 0..PHYS {
                        let seg = FRAME_SEG + (p * PAGE / 16) as u16;
                        bus.set_word(dst + p * 4, seg);
                        bus.set_word(dst + p * 4 + 2, p as u16);
                    }
                }
                cpu.r[CX] = PHYS as u16;
                0
            }
            _ => {
                self.unsupported.push((ah, al));
                0x84
            }
        };
        cpu.set8(4, status);
        if matches!(ah, 0x43..=0x45 | 0x50 | 0x51 | 0x57) && self.ops.len() < 4096 {
            self.ops.push(EmsOp {
                step: cpu.steps,
                ah,
                al,
                handle: cpu.r[DX],
                value: cpu.r[BX],
                status,
            });
        }
    }

    /// AH=43h。0 頁回 87h 沿 Go 既有行為（LIM 手冊寫 89h，Go 標為假說待驗）。
    fn alloc(&mut self, cpu: &mut Cpu) -> u8 {
        let want = usize::from(cpu.r[BX]);
        if want == 0 || want > TOTAL {
            return 0x87;
        }
        if want > TOTAL - self.used() {
            return 0x88;
        }
        let id = self.next;
        self.next += 1;
        self.handles.insert(id, vec![vec![0; PAGE]; want]);
        cpu.r[DX] = id;
        0
    }
    /// AH=44h。BX=FFFFh 解除映射。
    fn map(&mut self, cpu: &mut Cpu, bus: &mut Memory) -> u8 {
        let p = usize::from(cpu.reg8(0));
        if p >= PHYS {
            return 0x8b;
        }
        let (handle, logical) = (cpu.r[DX], cpu.r[BX]);
        let Some(count) = self.handles.get(&handle).map(Vec::len) else {
            return 0x83;
        };
        self.flush(bus, p);
        if logical == 0xffff {
            self.frame[p] = None;
            return 0;
        }
        if usize::from(logical) >= count {
            return 0x8a;
        }
        self.load(bus, p, handle, logical);
        0
    }
    /// AH=45h。釋放前把還映著的頁寫回。
    fn release(&mut self, cpu: &mut Cpu, bus: &mut Memory) -> u8 {
        let id = cpu.r[DX];
        if id == 0 || !self.handles.contains_key(&id) {
            return 0x83;
        }
        for p in 0..PHYS {
            if self.frame[p].is_some_and(|m| m.handle == id) {
                self.flush(bus, p);
                self.frame[p] = None
            }
        }
        self.handles.remove(&id);
        0
    }
    /// AH=4Eh。對映表格式為本庫自訂：每實體頁 4 bytes（handle、邏輯頁），規格上是 opaque。
    fn page_map(&mut self, cpu: &mut Cpu, bus: &mut Memory, al: u8) -> u8 {
        const ENTRY: usize = 4;
        match al {
            0 | 2 => {
                let dst = Cpu::address(cpu.s[ES], cpu.r[DI]);
                for p in 0..PHYS {
                    let (h, page) = self.frame[p].map_or((0xffff, 0xffff), |m| (m.handle, m.page));
                    bus.set_word(dst + p * ENTRY, h);
                    bus.set_word(dst + p * ENTRY + 2, page);
                }
                if al == 0 {
                    return 0;
                }
                self.restore_map(cpu, bus)
            }
            1 => self.restore_map(cpu, bus),
            3 => {
                cpu.set8(0, (PHYS * ENTRY) as u8);
                0
            }
            _ => {
                self.unsupported.push((0x4e, al));
                0x8f
            }
        }
    }
    fn restore_map(&mut self, cpu: &Cpu, bus: &mut Memory) -> u8 {
        let src = Cpu::address(cpu.s[DS], cpu.r[SI]);
        // 先寫回再重映，否則被換掉那一頁的修改會留在頁框裡被寫進新的那一頁。
        self.flush_all(bus);
        for p in 0..PHYS {
            let h = bus.word(src + p * 4);
            let page = bus.word(src + p * 4 + 2);
            if h == 0xffff {
                self.frame[p] = None;
                continue;
            }
            if !self
                .handles
                .get(&h)
                .is_some_and(|x| usize::from(page) < x.len())
            {
                return 0xa3;
            }
            self.load(bus, p, h, page)
        }
        0
    }
    /// AH=50h。先全部檢查再全部套用，中途失敗保持原狀。
    fn map_multiple(&mut self, cpu: &mut Cpu, bus: &mut Memory, al: u8) -> u8 {
        let handle = cpu.r[DX];
        let Some(count) = self.handles.get(&handle).map(Vec::len) else {
            return 0x83;
        };
        let src = Cpu::address(cpu.s[DS], cpu.r[SI]);
        let mut list = Vec::new();
        for i in 0..usize::from(cpu.r[CX]) {
            let logical = bus.word(src + i * 4);
            let mut phys = bus.word(src + i * 4 + 2);
            if al == 1 {
                if phys < FRAME_SEG {
                    return 0x8b;
                }
                phys = (phys - FRAME_SEG) / (PAGE / 16) as u16;
            }
            if usize::from(phys) >= PHYS {
                return 0x8b;
            }
            if logical != 0xffff && usize::from(logical) >= count {
                return 0x8a;
            }
            list.push((logical, usize::from(phys)));
        }
        for (logical, p) in list {
            self.flush(bus, p);
            if logical == 0xffff {
                self.frame[p] = None
            } else {
                self.load(bus, p, handle, logical)
            }
        }
        0
    }
    /// AH=51h。縮小保留前面的頁；被砍掉且映著的頁先寫回並解除映射。
    fn realloc(&mut self, cpu: &mut Cpu, bus: &mut Memory) -> u8 {
        let handle = cpu.r[DX];
        let Some(have) = self.handles.get(&handle).map(Vec::len) else {
            return 0x83;
        };
        let want = usize::from(cpu.r[BX]);
        if want > TOTAL - self.used() + have {
            cpu.r[BX] = have as u16;
            return 0x88;
        }
        for p in 0..PHYS {
            if self.frame[p].is_some_and(|m| m.handle == handle && usize::from(m.page) >= want) {
                self.flush(bus, p);
                self.frame[p] = None
            }
        }
        self.handles
            .get_mut(&handle)
            .unwrap()
            .resize(want, vec![0; PAGE]);
        0
    }
    fn frame_of(&self, handle: u16, page: u16) -> Option<usize> {
        self.frame
            .iter()
            .position(|m| *m == Some(Mapping { handle, page }))
    }
    fn region(bus: &Memory, at: usize) -> (bool, u16, usize) {
        let ems = bus.ram[at] != 0;
        let handle = bus.word(at + 1);
        let off = usize::from(bus.word(at + 3));
        let seg_or_page = usize::from(bus.word(at + 5));
        let base = if ems {
            seg_or_page * PAGE + off
        } else {
            seg_or_page * 16 + off
        };
        (ems, handle, base)
    }
    fn region_check(&self, r: (bool, u16, usize), n: usize) -> u8 {
        if !r.0 {
            return 0;
        }
        match self.handles.get(&r.1) {
            None => 0x83,
            Some(h) if r.2 + n > h.len() * PAGE => 0x8a,
            _ => 0,
        }
    }
    fn region_read(&self, bus: &Memory, r: (bool, u16, usize), i: usize) -> u8 {
        if !r.0 {
            return bus.ram[(r.2 + i) & 0xfffff];
        }
        let lin = r.2 + i;
        let (page, off) = ((lin / PAGE) as u16, lin % PAGE);
        match self.frame_of(r.1, page) {
            Some(p) => bus.ram[frame_addr(p) + off],
            None => self.handles[&r.1][usize::from(page)][off],
        }
    }
    fn region_write(&mut self, bus: &mut Memory, r: (bool, u16, usize), i: usize, v: u8) {
        if !r.0 {
            bus.ram[(r.2 + i) & 0xfffff] = v;
            return;
        }
        let lin = r.2 + i;
        let (page, off) = ((lin / PAGE) as u16, lin % PAGE);
        match self.frame_of(r.1, page) {
            Some(p) => bus.ram[frame_addr(p) + off] = v,
            None => self.handles.get_mut(&r.1).unwrap()[usize::from(page)][off] = v,
        }
    }
    /// AH=57h。AL=00 搬移、01 交換；重疊時先整段讀出再寫。
    fn move_region(&mut self, cpu: &mut Cpu, bus: &mut Memory, al: u8) -> u8 {
        if al > 1 {
            self.unsupported.push((0x57, al));
            return 0x84;
        }
        let desc = Cpu::address(cpu.s[DS], cpu.r[SI]);
        let n = usize::from(bus.word(desc)) | (usize::from(bus.word(desc + 2)) << 16);
        let (src, dst) = (Self::region(bus, desc + 4), Self::region(bus, desc + 0x0b));
        let status = if n > 1 << 20 {
            0x96
        } else {
            match self.region_check(src, n) {
                0 => self.region_check(dst, n),
                s => s,
            }
        };
        if status != 0 {
            return status;
        }
        let a: Vec<u8> = (0..n).map(|i| self.region_read(bus, src, i)).collect();
        if al == 1 {
            let b: Vec<u8> = (0..n).map(|i| self.region_read(bus, dst, i)).collect();
            for (i, v) in b.into_iter().enumerate() {
                self.region_write(bus, src, i, v)
            }
        }
        for (i, v) in a.into_iter().enumerate() {
            self.region_write(bus, dst, i, v)
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::AX;

    fn call(e: &mut Ems, cpu: &mut Cpu, bus: &mut Memory, ax: u16) -> u8 {
        cpu.r[AX] = ax;
        e.call(cpu, bus);
        cpu.reg8(4)
    }

    #[test]
    fn header_signature_and_vector() {
        let mut bus = Memory::default();
        install(&mut bus);
        assert_eq!(bus.word(0x67 * 4 + 2), HEADER_SEG);
        let base = usize::from(HEADER_SEG) * 16;
        assert_eq!(&bus.ram[base + 0x0a..base + 0x12], b"EMMXXXX0");
        assert_eq!(&bus.ram[base..base + 3], &[0xcd, TRAMPOLINE, 0xcf]);
    }

    #[test]
    fn frame_counts_and_alloc_errors() {
        let (mut e, mut cpu, mut bus) = (Ems::new(), Cpu::default(), Memory::default());
        assert_eq!(call(&mut e, &mut cpu, &mut bus, 0x4100), 0);
        assert_eq!(cpu.r[BX], FRAME_SEG);
        cpu.r[BX] = 32;
        assert_eq!(call(&mut e, &mut cpu, &mut bus, 0x4300), 0);
        assert_eq!(cpu.r[DX], 1);
        assert_eq!(call(&mut e, &mut cpu, &mut bus, 0x4200), 0);
        assert_eq!((cpu.r[BX], cpu.r[DX]), (480, 512));
        cpu.r[BX] = 0;
        assert_eq!(call(&mut e, &mut cpu, &mut bus, 0x4300), 0x87);
        cpu.r[BX] = 481;
        assert_eq!(call(&mut e, &mut cpu, &mut bus, 0x4300), 0x88);
        cpu.r[BX] = 513;
        assert_eq!(call(&mut e, &mut cpu, &mut bus, 0x4300), 0x87);
        assert_eq!(call(&mut e, &mut cpu, &mut bus, 0x5a00), 0x84);
        assert_eq!(e.unsupported, vec![(0x5a, 0)]);
    }

    #[test]
    fn remap_writes_back_before_switching() {
        let (mut e, mut cpu, mut bus) = (Ems::new(), Cpu::default(), Memory::default());
        cpu.r[BX] = 2;
        call(&mut e, &mut cpu, &mut bus, 0x4300);
        let h = cpu.r[DX];
        let base = frame_addr(0);
        for (logical, v) in [(0u16, 0x11u8), (1, 0x22)] {
            cpu.r[BX] = logical;
            cpu.r[DX] = h;
            assert_eq!(call(&mut e, &mut cpu, &mut bus, 0x4400), 0);
            bus.ram[base + 5] = v;
        }
        cpu.r[BX] = 0;
        cpu.r[DX] = h;
        call(&mut e, &mut cpu, &mut bus, 0x4400);
        assert_eq!(bus.ram[base + 5], 0x11, "換回第 0 頁要拿到寫回的內容");
        cpu.r[BX] = 1;
        cpu.r[DX] = h;
        call(&mut e, &mut cpu, &mut bus, 0x4400);
        assert_eq!(bus.ram[base + 5], 0x22);
        // 負對照：邏輯頁超範圍與無效 handle 不改頁框。
        cpu.r[BX] = 2;
        cpu.r[DX] = h;
        assert_eq!(call(&mut e, &mut cpu, &mut bus, 0x4400), 0x8a);
        cpu.r[DX] = 9;
        assert_eq!(call(&mut e, &mut cpu, &mut bus, 0x4400), 0x83);
        assert_eq!(bus.ram[base + 5], 0x22);
    }

    #[test]
    fn map_multiple_is_atomic() {
        let (mut e, mut cpu, mut bus) = (Ems::new(), Cpu::default(), Memory::default());
        cpu.r[BX] = 2;
        call(&mut e, &mut cpu, &mut bus, 0x4300);
        let h = cpu.r[DX];
        let src = 0x1000;
        bus.set_word(src, 0);
        bus.set_word(src + 2, 0);
        bus.set_word(src + 4, 5); // 第二組邏輯頁超範圍
        bus.set_word(src + 6, 1);
        cpu.s[DS] = 0x100;
        cpu.r[SI] = 0;
        cpu.r[CX] = 2;
        cpu.r[DX] = h;
        assert_eq!(call(&mut e, &mut cpu, &mut bus, 0x5000), 0x8a);
        assert_eq!(e.frame, [None; PHYS], "失敗時第一組也不能套用");
        bus.set_word(src + 4, 1);
        cpu.r[DX] = h;
        assert_eq!(call(&mut e, &mut cpu, &mut bus, 0x5000), 0);
        assert_eq!(e.frame[0], Some(Mapping { handle: h, page: 0 }));
        assert_eq!(e.frame[1], Some(Mapping { handle: h, page: 1 }));
    }

    #[test]
    fn move_region_to_unmapped_page() {
        let (mut e, mut cpu, mut bus) = (Ems::new(), Cpu::default(), Memory::default());
        cpu.r[BX] = 1;
        call(&mut e, &mut cpu, &mut bus, 0x4300);
        let h = cpu.r[DX];
        bus.ram[0x2000..0x2004].copy_from_slice(&[1, 2, 3, 4]);
        let d = 0x3000;
        bus.set_word(d, 4);
        bus.set_word(d + 2, 0);
        bus.ram[d + 4] = 0; // 來源：常規記憶體 0200:0000
        bus.set_word(d + 7, 0);
        bus.set_word(d + 9, 0x200);
        bus.ram[d + 0x0b] = 1; // 目的：handle h 第 0 頁位移 8
        bus.set_word(d + 0x0c, h);
        bus.set_word(d + 0x0e, 8);
        bus.set_word(d + 0x10, 0);
        cpu.s[DS] = 0x300;
        cpu.r[SI] = 0;
        assert_eq!(call(&mut e, &mut cpu, &mut bus, 0x5700), 0);
        assert_eq!(&e.handles[&h][0][8..12], &[1, 2, 3, 4]);
    }
}
