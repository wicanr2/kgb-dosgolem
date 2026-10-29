// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 譯文表 `text/*.zh-TW.tsv`：三欄 key、translation、source，UTF-8。
//! key 是原版英文（比對鍵，原樣保留 `^`）；translation 以 `\n` 表示強制換行；
//! source 記出處（呼叫點、檔案）。以 `#` 開頭的行是註解。
use std::collections::HashMap;

#[derive(Default)]
pub struct Catalog {
    map: HashMap<Vec<u8>, String>,
}

/// 原版位元組轉成 key 字串：可列印 ASCII 原樣，其餘寫成 `\xNN`。
pub fn key_of(text: &[u8]) -> String {
    let mut s = String::new();
    for &b in text {
        match b {
            b'\\' => s.push_str("\\\\"),
            0x20..=0x7e => s.push(char::from(b)),
            _ => s.push_str(&format!("\\x{b:02x}")),
        }
    }
    s
}

fn key_bytes(key: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut it = key.bytes();
    while let Some(b) = it.next() {
        if b != b'\\' {
            out.push(b);
            continue;
        }
        match it.next() {
            Some(b'\\') => out.push(b'\\'),
            Some(b'x') => {
                let h: Vec<u8> = it.by_ref().take(2).collect();
                let s = std::str::from_utf8(&h).map_err(|e| e.to_string())?;
                out.push(u8::from_str_radix(s, 16).map_err(|e| format!("{key}: {e}"))?);
            }
            other => return Err(format!("{key}: 未知跳脫 {other:?}")),
        }
    }
    Ok(out)
}

impl Catalog {
    pub fn parse(tsv: &str) -> Result<Self, String> {
        let mut map = HashMap::new();
        for (n, line) in tsv.lines().enumerate() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() != 3 {
                return Err(format!("第 {} 行欄數 {}，應為 3", n + 1, cols.len()));
            }
            let key = key_bytes(cols[0])?;
            if cols[1].is_empty() {
                continue;
            }
            if map.insert(key, cols[1].replace("\\n", "\n")).is_some() {
                return Err(format!("第 {} 行 key 重複：{}", n + 1, cols[0]));
            }
        }
        Ok(Self { map })
    }
    pub fn get(&self, text: &[u8]) -> Option<&str> {
        self.map.get(text).map(String::as_str)
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    /// 譯文用到的全部字元（產生字型子集與缺字檢查用）。
    pub fn chars(&self) -> std::collections::BTreeSet<char> {
        self.map.values().flat_map(|s| s.chars()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_errors() {
        let raw = b"Hi^there\x82\\";
        let k = key_of(raw);
        assert_eq!(k, "Hi^there\\x82\\\\");
        let c = Catalog::parse(&format!("# c\n{k}\t你好\\n世界\tintro\nNope\t\tx\n")).unwrap();
        assert_eq!(c.get(raw), Some("你好\n世界"));
        assert_eq!(c.get(b"Nope"), None, "空譯文視為未譯");
        assert!(Catalog::parse("a\tb\n").is_err(), "欄數不符要失敗");
        assert!(Catalog::parse("a\tb\tc\na\td\te\n").is_err(), "key 重複要失敗");
    }
}
