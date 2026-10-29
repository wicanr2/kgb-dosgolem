// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
// Experimental real-mode DOS owner. Contracts ported from RRSAL Go dosgolem.
// Source: https://github.com/wicanr2/dosgolem at f79eab6; see docs/native-core.md.
use super::{cpu::*, memory::Memory, mz};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
#[path = "machine_services.rs"]
mod services;

/// 一次開檔：步數、AH、DOS 路徑、結果（handle，或負的 DOS 錯誤碼）。
#[derive(Clone, Debug)]
pub struct FileEvent {
    pub step: u64,
    pub ah: u8,
    pub name: String,
    pub result: i32,
}

#[derive(Clone)]
struct Block {
    size: u16,
    owner: u16,
}
struct Parent {
    cpu: Cpu,
    psp: u16,
    name: String,
    vectors: [u16; 6],
}
pub struct Machine {
    pub cpu: Cpu,
    pub bus: Memory,
    pub root: PathBuf,
    pub program: String,
    pub psp: u16,
    pub started: u64,
    pub events: Vec<Value>,
    pub console: Vec<u8>,
    pub exited: bool,
    pub blocked: bool,
    pub services: BTreeMap<String, u64>,
    pub dos_events: Vec<Value>,
    pub ems: crate::ems::Ems,
    /// 開檔紀錄（AH=3Ch／3Dh），上限 4096 筆。
    pub file_log: Vec<FileEvent>,
    blocks: BTreeMap<u16, Block>,
    parents: Vec<Parent>,
    handles: BTreeMap<u16, File>,
    keys: VecDeque<u16>,
    dta: (u16, u16),
    last_exit: u16,
    last_error: u16,
    irq_at: u64,
    irq0_pending: bool,
    pub irq7_delivered: u64,
    search: VecDeque<PathBuf>,
}
impl Machine {
    pub fn new(root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        if !root.is_dir() {
            return Err("DOS root must be a directory".into());
        }
        let mut m = Self {
            cpu: Cpu::default(),
            bus: Memory::default(),
            root,
            program: String::new(),
            psp: 0x187b,
            started: 0,
            events: Vec::new(),
            console: Vec::new(),
            exited: false,
            blocked: false,
            services: BTreeMap::new(),
            dos_events: Vec::new(),
            ems: crate::ems::Ems::new(),
            file_log: Vec::new(),
            blocks: BTreeMap::new(),
            parents: Vec::new(),
            handles: BTreeMap::new(),
            keys: VecDeque::new(),
            dta: (0x187b, 0x80),
            last_exit: 0,
            last_error: 0,
            irq_at: 262144,
            irq0_pending: false,
            irq7_delivered: 0,
            search: VecDeque::new(),
        };
        m.bus.set_word(0x410, 0x0021);
        m.bus.set_word(0x413, 640);
        m.bus.set_word(0x41a, 0x1e);
        m.bus.set_word(0x41c, 0x1e);
        m.bus.set_word(0x480, 0x1e);
        m.bus.set_word(0x482, 0x3e);
        m.bus.ram[0x449] = 3;
        m.bus.set_word(0x44a, 80);
        m.bus.set_word(0x44c, 0x1000);
        m.bus.ram[0x484] = 24;
        for n in 0..256 {
            m.bus.set_word(n * 4, n as u16 * 4);
            m.bus.set_word(n * 4 + 2, 0x80);
            let p = 0x800 + n * 4;
            m.bus.ram[p..p + 3].copy_from_slice(&[0xcd, n as u8, 0xcf])
        }
        m.bus.ram[0x800 + 8 * 4] = 0xcf;
        m.bus.ram[0x800 + 0x1c * 4] = 0xcf;
        crate::ems::install(&mut m.bus);
        m.blocks.insert(
            0xcf,
            Block {
                size: 0x187a - 0xd0,
                owner: m.psp,
            },
        );
        m.blocks.insert(
            m.psp - 1,
            Block {
                size: 0xa000 - m.psp,
                owner: m.psp,
            },
        );
        m.sync_blocks();
        Ok(m)
    }
    pub fn load(&mut self, name: &str) -> Result<(), String> {
        self.load_at(name, self.psp)?;
        Ok(())
    }
    /// 載入並寫入 PSP 80h 的命令列尾：長度、內容（通常以空白開頭）、0Dh。
    pub fn load_with_tail(&mut self, name: &str, tail: &[u8]) -> Result<(), String> {
        if tail.len() > 126 || tail.contains(&13) {
            return Err("DOS command tail must be at most 126 bytes without CR".into());
        }
        self.load_at(name, self.psp)?;
        let base = usize::from(self.psp) * 16;
        self.bus.ram[base + 0x80] = tail.len() as u8;
        self.bus.ram[base + 0x81..base + 0x81 + tail.len()].copy_from_slice(tail);
        self.bus.ram[base + 0x81 + tail.len()] = 13;
        Ok(())
    }
    fn resolve(&self, name: &str, create: bool) -> Result<PathBuf, String> {
        let normalized = name.replace('\\', "/");
        let normalized = if normalized.as_bytes().get(1) == Some(&b':') {
            &normalized[2..]
        } else {
            &normalized
        };
        let mut path = self.root.clone();
        for component in Path::new(normalized.trim_start_matches('/')).components() {
            match component {
                Component::Normal(part) => {
                    let text = part.to_string_lossy();
                    let found = fs::read_dir(&path)
                        .map_err(|e| e.to_string())?
                        .filter_map(Result::ok)
                        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(&text))
                        .map(|e| e.path());
                    path = found.unwrap_or_else(|| path.join(part));
                    if path.exists() {
                        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                        if metadata.file_type().is_symlink() {
                            return Err("DOS source symlink rejected".into());
                        }
                    }
                }
                Component::CurDir => {}
                _ => return Err("DOS path traversal rejected".into()),
            }
        }
        if !create && !path.exists() {
            return Err("DOS file not found".into());
        }
        Ok(path)
    }
    fn cstring(&self, seg: u16, off: u16) -> Result<String, String> {
        let mut bytes = Vec::new();
        for i in 0..65536 {
            let b = self.bus.ram[Cpu::address(seg, off.wrapping_add(i as u16))];
            if b == 0 {
                return String::from_utf8(bytes)
                    .map_err(|_| "non-ASCII DOS path unsupported".into());
            }
            bytes.push(b)
        }
        Err("unterminated DOS path".into())
    }
    fn load_at(&mut self, name: &str, psp: u16) -> Result<(), String> {
        let path = self.resolve(name, false)?;
        let raw = fs::read(path).map_err(|e| e.to_string())?;
        let load = psp.checked_add(16).ok_or("load segment overflow")?;
        let image = mz::decode(&raw, load)?;
        let start = usize::from(load) * 16;
        if start + image.bytes.len() > 0xa0000 {
            return Err("DOS image exceeds conventional memory".into());
        }
        self.bus.ram[start..start + image.bytes.len()].copy_from_slice(&image.bytes);
        let base = usize::from(psp) * 16;
        self.bus.ram[base..base + 256].fill(0);
        self.bus.ram[base..base + 2].copy_from_slice(&[0xcd, 0x20]);
        self.bus.set_word(base + 2, 0xa000);
        self.bus.set_word(base + 0x16, self.psp);
        self.bus.set_word(base + 0x2c, 0xd0);
        self.bus.set_word(base + 0x32, 20);
        self.bus.set_word(base + 0x34, 0x18);
        self.bus.set_word(base + 0x36, psp);
        self.bus.ram[base + 0x18..base + 0x2c].fill(255);
        for i in 0..5 {
            self.bus.ram[base + 0x18 + i] = i as u8
        }
        self.bus.ram[base + 0x81] = 13;
        let environment = format!("COMSPEC=C:\\COMMAND.COM\0\0\u{1}\0C:\\{name}\0");
        self.bus.ram[0xd00..0xd00 + environment.len()].copy_from_slice(environment.as_bytes());
        let cycles = self.cpu.cycles;
        let steps = self.cpu.steps;
        self.cpu = Cpu::default();
        self.cpu.cycles = cycles;
        self.cpu.steps = steps;
        self.cpu.s = [
            psp,
            if image.com {
                psp
            } else {
                load.wrapping_add(image.cs)
            },
            if image.com {
                psp
            } else {
                load.wrapping_add(image.ss)
            },
            psp,
        ];
        self.cpu.ip = image.ip;
        self.cpu.r[SP] = image.sp;
        self.cpu.set_flags(IF);
        self.psp = psp;
        self.program = name.to_ascii_uppercase();
        self.dta = (psp, 0x80);
        self.started += 1;
        self.events.push(json!({"name":self.program,"psp":psp,"load_segment":load,"entry":Cpu::address(self.cpu.s[CS],self.cpu.ip),"entry_cs_ip":[self.cpu.s[CS],self.cpu.ip],"started":self.started,"relocations":image.relocations}));
        Ok(())
    }
    fn sync_blocks(&mut self) {
        let last = self.blocks.keys().next_back().copied();
        for (&seg, b) in &self.blocks {
            let at = usize::from(seg) * 16;
            self.bus.ram[at] = if Some(seg) == last { b'Z' } else { b'M' };
            self.bus.set_word(at + 1, b.owner);
            self.bus.set_word(at + 3, b.size);
        }
    }
    fn allocate(&mut self, need: u16) -> Option<u16> {
        let seg = *self
            .blocks
            .iter()
            .find(|(_, b)| b.owner == 0 && b.size >= need)?
            .0;
        let old = self.blocks.remove(&seg).unwrap();
        self.blocks.insert(
            seg,
            Block {
                size: need,
                owner: self.psp,
            },
        );
        if old.size > need {
            self.blocks.insert(
                seg + need + 1,
                Block {
                    size: old.size - need - 1,
                    owner: 0,
                },
            );
        }
        self.sync_blocks();
        Some(seg + 1)
    }
    fn merge_free(&mut self) {
        let keys: Vec<u16> = self.blocks.keys().copied().collect();
        for pair in keys.windows(2).rev() {
            let (a, b) = (pair[0], pair[1]);
            if self.blocks.get(&a).is_some_and(|x| x.owner == 0)
                && self.blocks.get(&b).is_some_and(|x| x.owner == 0)
            {
                let tail = self.blocks.remove(&b).unwrap();
                self.blocks.get_mut(&a).unwrap().size += tail.size + 1
            }
        }
        self.sync_blocks();
    }
    fn resize(&mut self, seg: u16, need: u16) -> bool {
        let Some(old) = self.blocks.get(&seg).cloned() else {
            return false;
        };
        let next = seg + old.size + 1;
        let extra = self
            .blocks
            .get(&next)
            .filter(|b| b.owner == 0)
            .map(|b| b.size + 1)
            .unwrap_or(0);
        if u32::from(need) > u32::from(old.size) + u32::from(extra) {
            return false;
        }
        if extra > 0 {
            self.blocks.remove(&next);
        }
        self.blocks.get_mut(&seg).unwrap().size = need;
        let remain = u32::from(old.size) + u32::from(extra) - u32::from(need);
        if remain > 0 {
            self.blocks.insert(
                seg + need + 1,
                Block {
                    size: remain as u16 - 1,
                    owner: 0,
                },
            );
        }
        self.merge_free();
        true
    }
    pub fn status(&self) -> Value {
        json!({"core":"xaika-native","executed":self.cpu.steps,"ticks":self.cpu.cycles,
        "registers":{"ax":self.cpu.r[AX],"cx":self.cpu.r[CX],"dx":self.cpu.r[DX],"bx":self.cpu.r[BX],"sp":self.cpu.r[SP],"bp":self.cpu.r[BP],"si":self.cpu.r[SI],"di":self.cpu.r[DI],"es":self.cpu.s[ES],"cs":self.cpu.s[CS],"ss":self.cpu.s[SS],"ds":self.cpu.s[DS],"ip":self.cpu.ip,"flags":self.cpu.flags},
        "program":{"name":self.program,"psp":self.psp,"started":self.started},"exited":self.exited,"blocked":self.blocked,"video_mode":self.bus.vga.mode,"services":self.services,"events":self.events,"dos_events":self.dos_events,"memory_blocks":self.blocks.iter().map(|(s,b)|json!({"mcb":s,"size":b.size,"owner":b.owner})).collect::<Vec<_>>()})
    }
    pub fn step(&mut self) -> Result<(), String> {
        if self.exited {
            return Ok(());
        }
        self.bus.cycles = self.cpu.cycles;
        if self.cpu.cycles >= self.irq_at {
            self.irq_at = self.cpu.cycles + u64::from(self.bus.pit_divisor) * 4;
            // 先掛起、等 IF 與 IMR 允許再送（Go machine.go 的 irq0Pending 語意，docs/spec/003 §4）。
            self.irq0_pending = true;
        }
        self.bus.tick_devices();
        if self.cpu.flag(IF) {
            if self.irq0_pending && self.bus.pic_mask & 1 == 0 {
                self.irq0_pending = false;
                let ticks =
                    u32::from(self.bus.word(0x46c)) | (u32::from(self.bus.word(0x46e)) << 16);
                let ticks = ticks.wrapping_add(1);
                self.bus.set_word(0x46c, ticks as u16);
                self.bus.set_word(0x46e, (ticks >> 16) as u16);
                let off = self.bus.word(8 * 4);
                let seg = self.bus.word(8 * 4 + 2);
                if (seg, off) != (0x80, 32) {
                    self.cpu.interrupt(&mut self.bus, 8)
                } else if (self.bus.word(0x1c * 4 + 2), self.bus.word(0x1c * 4)) != (0x80, 0x70) {
                    self.cpu.interrupt(&mut self.bus, 0x1c)
                }
            } else if self.bus.irq7_pending && self.bus.pic_mask & 0x80 == 0 {
                self.bus.irq7_pending = false;
                // 向量仍是 stub＝沒有程式接手，照 BIOS 預設丟棄。
                if self.bus.word(0x0f * 4 + 2) != 0x80 {
                    self.irq7_delivered += 1;
                    self.cpu.interrupt(&mut self.bus, 0x0f)
                }
            }
        }
        if let Some(n) = self.cpu.step(&mut self.bus)? {
            self.service(n)?
        }
        if let Some(error) = self.bus.port_error.take() {
            return Err(format!(
                "{error} at {:04x}:{:04x}",
                self.cpu.start.0, self.cpu.start.1
            ));
        }
        Ok(())
    }
    pub fn key(&mut self, scan: u8, ascii: u8, down: bool) {
        self.bus
            .scans
            .push_back(if down { scan } else { scan | 0x80 });
        if !down {
            return;
        }
        let tail = self.bus.word(0x41c);
        let next = if tail + 2 >= 0x3e { 0x1e } else { tail + 2 };
        if next != self.bus.word(0x41a) {
            self.bus.set_word(
                0x400 + usize::from(tail),
                (u16::from(scan) << 8) | u16::from(ascii),
            );
            self.bus.set_word(0x41c, next)
        }
    }
    fn peek_key(&self) -> Option<u16> {
        let head = self.bus.word(0x41a);
        if head != self.bus.word(0x41c) {
            Some(self.bus.word(0x400 + usize::from(head)))
        } else {
            self.keys.front().copied()
        }
    }
    fn pop_key(&mut self) -> Option<u16> {
        let head = self.bus.word(0x41a);
        if head != self.bus.word(0x41c) {
            let v = self.bus.word(0x400 + usize::from(head));
            self.bus
                .set_word(0x41a, if head + 2 >= 0x3e { 0x1e } else { head + 2 });
            Some(v)
        } else {
            self.keys.pop_front()
        }
    }
    fn fail(&mut self, code: u16) {
        self.cpu.r[AX] = code;
        self.last_error = code;
        self.cpu.set_flag(CF, true)
    }
    fn terminate(&mut self) {
        self.last_exit = u16::from(self.cpu.reg8(0));
        if let Some(parent) = self.parents.pop() {
            for b in self.blocks.values_mut() {
                if b.owner == self.psp {
                    b.owner = 0
                }
            }
            self.merge_free();
            let cycles = self.cpu.cycles;
            let steps = self.cpu.steps;
            self.cpu = parent.cpu;
            self.cpu.cycles = cycles;
            self.cpu.steps = steps;
            self.psp = parent.psp;
            self.program = parent.name;
            for (i, n) in [0x22, 0x23, 0x24].iter().enumerate() {
                self.bus.set_word(n * 4, parent.vectors[i * 2]);
                self.bus.set_word(n * 4 + 2, parent.vectors[i * 2 + 1])
            }
            self.cpu.set_flag(CF, false);
        } else {
            self.exited = true;
        }
    }
    fn service(&mut self, n: u8) -> Result<(), String> {
        let ah = self.cpu.reg8(4);
        let al = self.cpu.reg8(0);
        *self
            .services
            .entry(format!("{n:02x}/{ah:02x}"))
            .or_default() += 1;
        match n {
            0x20 => self.terminate(),
            0x21 => {
                let before = self.cpu.clone();
                let name = self.program.clone();
                self.dos(ah, al)?;
                if matches!(ah, 0x48..=0x4c) && self.dos_events.len() < 512 {
                    self.dos_events.push(json!({"program":name,"step":before.steps,"ah":ah,"al":al,"cs":before.start.0,"ip":before.start.1,"bx_in":before.r[BX],"es_in":before.s[ES],"ax_out":self.cpu.r[AX],"bx_out":self.cpu.r[BX],"cf":self.cpu.flag(CF)}));
                }
            }
            0x10 => self.video(ah, al)?,
            0x11 => self.cpu.r[AX] = self.bus.word(0x410),
            0x12 => self.cpu.r[AX] = 640,
            0x16 => match ah {
                0 | 0x10 => {
                    if let Some(v) = self.pop_key() {
                        self.cpu.r[AX] = v;
                        self.blocked = false
                    } else {
                        self.blocked = true;
                        self.cpu.s[CS] = self.cpu.start.0;
                        self.cpu.ip = self.cpu.start.1
                    }
                }
                1 | 0x11 => {
                    let v = self.peek_key();
                    self.cpu.set_flag(ZF, v.is_none());
                    if let Some(v) = v {
                        self.cpu.r[AX] = v
                    }
                }
                2 | 0x12 => self.cpu.set8(0, 0),
                _ => return Err(format!("unsupported BIOS keyboard {ah:02x}")),
            },
            0x1a => match ah {
                0 => {
                    self.cpu.r[CX] = self.bus.word(0x46e);
                    self.cpu.r[DX] = self.bus.word(0x46c);
                    self.cpu.set8(0, 0)
                }
                1 => {
                    self.bus.set_word(0x46c, self.cpu.r[DX]);
                    self.bus.set_word(0x46e, self.cpu.r[CX])
                }
                2 | 4 => self.cpu.set_flag(CF, true),
                _ => return Err(format!("unsupported clock function {ah:02x}")),
            },
            0x2f if self.cpu.r[AX] == 0x4300 => self.cpu.set8(0, 0),
            0x33 if self.cpu.r[AX] == 0 => {
                self.cpu.r[AX] = 0;
                self.cpu.r[BX] = 0
            }
            crate::ems::TRAMPOLINE => self.ems.call(&mut self.cpu, &mut self.bus),
            8 | 0x1c => {}
            _ => {
                let (seg, _off) = (
                    self.bus.word(usize::from(n) * 4 + 2),
                    self.bus.word(usize::from(n) * 4),
                );
                if seg == 0x80 {
                    return Err(format!(
                        "unsupported interrupt {n:02x}, AX={:04x}",
                        self.cpu.r[AX]
                    ));
                }
                self.cpu.interrupt(&mut self.bus, n)
            }
        }
        Ok(())
    }
}
