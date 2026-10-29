// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
use dosgolem_core::machine::Machine;
use std::fs;

fn machine_with_com(tag: &str) -> (Machine, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("dgrs-tail-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("T.COM"), [0xcd, 0x20]).unwrap();
    (Machine::new(&dir).unwrap(), dir)
}

#[test]
fn tail_is_written_with_length_and_cr() {
    let (mut m, dir) = machine_with_com("ok");
    m.load_with_tail("T.COM", b" EMS 386").unwrap();
    let base = usize::from(m.psp) * 16;
    assert_eq!(m.bus.ram[base + 0x80], 8);
    assert_eq!(&m.bus.ram[base + 0x81..base + 0x89], b" EMS 386");
    assert_eq!(m.bus.ram[base + 0x89], 13);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn plain_load_leaves_empty_tail() {
    // 負對照：不經 load_with_tail 時長度為 0，證明上一個測試量到的是新函式寫的值。
    let (mut m, dir) = machine_with_com("plain");
    m.load("T.COM").unwrap();
    let base = usize::from(m.psp) * 16;
    assert_eq!(m.bus.ram[base + 0x80], 0);
    assert_eq!(m.bus.ram[base + 0x81], 13);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn oversized_or_cr_tail_is_rejected() {
    let (mut m, dir) = machine_with_com("bad");
    assert!(m.load_with_tail("T.COM", &[b'A'; 127]).is_err());
    assert!(m.load_with_tail("T.COM", b"A\rB").is_err());
    assert!(m.load_with_tail("T.COM", &[b'A'; 126]).is_ok());
    fs::remove_dir_all(dir).unwrap();
}
