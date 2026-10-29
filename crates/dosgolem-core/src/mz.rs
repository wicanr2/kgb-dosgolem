// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
// MZ bounds and relocation contract ported from Go dosgolem loader.go f79eab6.
// Original: Wang Chun-Yu / wicanr2, https://github.com/wicanr2/dosgolem .
#[derive(Debug)]
pub struct Image {
    pub bytes: Vec<u8>,
    pub cs: u16,
    pub ip: u16,
    pub ss: u16,
    pub sp: u16,
    pub min_extra: u16,
    pub max_extra: u16,
    pub relocations: usize,
    pub com: bool,
}
fn word(data: &[u8], offset: usize) -> Result<u16, String> {
    let b = data.get(offset..offset + 2).ok_or("truncated MZ word")?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}
pub fn decode(data: &[u8], relocation: u16) -> Result<Image, String> {
    if !data.starts_with(b"MZ") {
        if data.is_empty() || data.len() > 0xfefe {
            return Err("COM image exceeds one segment".into());
        }
        return Ok(Image {
            bytes: data.to_vec(),
            cs: 0,
            ip: 0x100,
            ss: 0,
            sp: 0xfffe,
            min_extra: 0,
            max_extra: 0xffff,
            relocations: 0,
            com: true,
        });
    }
    if data.len() < 28 {
        return Err("truncated MZ header".into());
    }
    let header = usize::from(word(data, 8)?) * 16;
    let pages = usize::from(word(data, 4)?);
    let last = usize::from(word(data, 2)? & 511);
    let total = if last == 0 {
        pages * 512
    } else {
        pages.checked_sub(1).ok_or("MZ has zero pages")? * 512 + last
    };
    if header < 28 || total <= header || total > data.len() {
        return Err("MZ image bounds invalid".into());
    }
    let count = usize::from(word(data, 6)?);
    let table = usize::from(word(data, 24)?);
    if table + count * 4 > data.len() {
        return Err("MZ relocation table truncated".into());
    }
    let mut bytes = data[header..total].to_vec();
    let mut applied = 0;
    for i in 0..count {
        let at = usize::from(word(data, table + i * 4 + 2)?) * 16
            + usize::from(word(data, table + i * 4)?);
        if at + 2 > bytes.len() {
            continue;
        }
        let relocated = word(&bytes, at)?.wrapping_add(relocation);
        bytes[at..at + 2].copy_from_slice(&relocated.to_le_bytes());
        applied += 1;
    }
    if count > 0 && applied == 0 {
        return Err("no valid MZ relocations".into());
    }
    Ok(Image {
        bytes,
        cs: word(data, 22)?,
        ip: word(data, 20)?,
        ss: word(data, 14)?,
        sp: word(data, 16)?,
        min_extra: word(data, 10)?,
        max_extra: word(data, 12)?,
        relocations: applied,
        com: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relocated_pointer_and_strict_truncation() {
        let mut raw = vec![0u8; 36];
        raw[..2].copy_from_slice(b"MZ");
        for (at, v) in [
            (2, 36u16),
            (4, 1),
            (6, 1),
            (8, 2),
            (16, 0xfffe),
            (24, 28),
            (32, 0x1234),
        ] {
            raw[at..at + 2].copy_from_slice(&v.to_le_bytes())
        }
        let image = decode(&raw, 0x2000).unwrap();
        assert_eq!(image.bytes, [0x34, 0x32, 0, 0]);
        assert_eq!(image.relocations, 1);
        assert!(decode(&raw[..35], 0x2000).is_err());
        raw[28..30].copy_from_slice(&0xffffu16.to_le_bytes());
        assert!(decode(&raw, 0x2000).is_err());
    }
    #[test]
    fn com_is_unrelocated_and_bounded() {
        let raw = [0xb8, 0, 0x4c, 0xcd, 0x21];
        let image = decode(&raw, 0x1234).unwrap();
        assert!(image.com);
        assert_eq!(image.bytes, raw);
        assert_eq!(image.ip, 0x100);
        assert!(decode(&[], 0).is_err());
        assert!(decode(&vec![0; 0xfeff], 0).is_err());
    }
}
