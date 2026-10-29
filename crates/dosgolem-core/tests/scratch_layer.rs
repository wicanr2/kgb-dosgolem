// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 唯讀根目錄加暫存層：寫入時複製、建立與刪除只落在暫存層。
use dosgolem_core::machine::Machine;
use std::fs;

#[test]
fn writes_land_in_scratch_and_root_is_untouched() {
    let base = std::env::temp_dir().join(format!("dgrs-scratch-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let (root, scratch) = (base.join("root"), base.join("scratch"));
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&scratch).unwrap();
    fs::write(root.join("DATA.BIN"), b"orig").unwrap();
    // 程式：開 DATA.BIN 讀寫（3D02）、寫 1 byte、關；建 KGB\NEW.SAV（3C）、關；刪 DATA.BIN（41）；結束。
    let prog: Vec<u8> = [
        &[0xb8, 0x02, 0x3d, 0xba, 0x40, 0x01, 0xcd, 0x21][..], // mov ax,3D02; mov dx,140h; int 21
        &[0x89, 0xc3, 0xb4, 0x40, 0xb9, 0x01, 0x00, 0xba, 0x60, 0x01, 0xcd, 0x21], // mov bx,ax; ah=40 cx=1 dx=160h
        &[0xb4, 0x3e, 0xcd, 0x21], // close
        &[0xb4, 0x3c, 0x31, 0xc9, 0xba, 0x50, 0x01, 0xcd, 0x21], // create KGB\NEW.SAV
        &[0x89, 0xc3, 0xb4, 0x3e, 0xcd, 0x21],
        &[0xb4, 0x41, 0xba, 0x40, 0x01, 0xcd, 0x21], // delete DATA.BIN → 05h
        &[0xa3, 0x70, 0x01], // mov [170h],ax
        &[0xcd, 0x20],
    ]
    .concat();
    let mut com = prog.clone();
    com.resize(0x40, 0x90);
    com.extend(b"DATA.BIN\0");
    com.resize(0x50, 0);
    com.extend(b"KGB\\NEW.SAV\0");
    com.resize(0x60, 0);
    com.push(b'X');
    com.resize(0x80, 0);
    fs::write(root.join("T.COM"), &com).unwrap();
    let mut m = Machine::with_scratch(&root, &scratch).unwrap();
    m.load("T.COM").unwrap();
    while !m.exited && m.cpu.steps < 1000 {
        m.step().unwrap();
    }
    assert!(m.exited);
    assert_eq!(fs::read(root.join("DATA.BIN")).unwrap(), b"orig", "原版不可被改");
    assert_eq!(fs::read(scratch.join("DATA.BIN")).unwrap(), b"Xrig");
    assert!(scratch.join("KGB/NEW.SAV").exists());
    assert!(!root.join("KGB").exists());
    let ax = m.bus.word(usize::from(m.psp) * 16 + 0x170);
    assert_eq!(ax, 5, "刪除只存在根目錄的原版檔要回拒絕存取");
    fs::remove_dir_all(base).unwrap();
}
