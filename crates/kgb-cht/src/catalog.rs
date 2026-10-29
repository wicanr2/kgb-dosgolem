// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 譯文表 `text/*.zh-TW.tsv`：三欄 key、translation、source，UTF-8。
//! translation 以 `\n` 表示強制換行；source 記出處（呼叫點、片語編號）。以 `#` 開頭的行是註解。
//!
//! key 是比對鍵（原版英文，原樣保留 `^`），有兩種寫法：
//! - 原文：`key_of` 的跳脫字串。只用於少量句子（開場字幕）。
//! - 雜湊：`fnv64:` 加 16 位十六進位，為比對鍵位元組的 FNV-1a 64 位元雜湊（`hash_key`）。
//!   章節片語用這種寫法，原版全文不進版控（AGENTS.md §8），英文只留在 workplace。
//!
//! 兩種寫法查找時一律換算成雜湊，因此同一句不論寫成哪種都算重複。
use std::collections::HashMap;

#[derive(Default)]
pub struct Catalog {
    map: HashMap<u64, String>,
}

/// FNV-1a 64 位元（offset basis cbf29ce484222325、prime 100000001b3）。
pub fn fnv64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// 雜湊寫法的 key：`fnv64:` 加 16 位小寫十六進位。
pub fn hash_key(bytes: &[u8]) -> String {
    format!("fnv64:{:016x}", fnv64(bytes))
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
            let key = match cols[0].strip_prefix("fnv64:") {
                Some(h) if h.len() == 16 => {
                    u64::from_str_radix(h, 16).map_err(|e| format!("第 {} 行雜湊 key：{e}", n + 1))?
                }
                Some(_) => return Err(format!("第 {} 行雜湊 key 應為 16 位十六進位", n + 1)),
                None => fnv64(&key_bytes(cols[0])?),
            };
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
        self.map.get(&fnv64(text)).map(String::as_str)
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

    #[test]
    fn hashed_keys_match_literal_keys() {
        // FNV-1a 64 的標準測試向量。
        assert_eq!(fnv64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv64(b"a"), 0xaf63_dc4c_8601_ec8c);
        let raw = b"Your first day^at Department P.";
        let c = Catalog::parse(&format!("{}\t第一天\tCHAP1U:460\n", hash_key(raw))).unwrap();
        assert_eq!(c.get(raw), Some("第一天"));
        assert_eq!(c.get(b"Your first day at Department P."), None, "^ 不同就是不同句");
        let both = format!("{}\tx\ta\n{}\ty\tb\n", key_of(raw), hash_key(raw));
        assert!(Catalog::parse(&both).is_err(), "原文與雜湊寫同一句要算重複");
        assert!(Catalog::parse("fnv64:123\tx\ta\n").is_err(), "雜湊長度不對要失敗");
    }
}
