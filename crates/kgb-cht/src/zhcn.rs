// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 繁中（台灣用語）轉簡中（大陸用語）：OpenCC `tw2sp`（ferrous-opencc 0.4.0，Apache-2.0）
//! 加譯名表 `text/glossary.tsv` 的簡中欄覆寫（AGENTS.md §1「兩個中文版本」）。
//!
//! 覆寫做法：譯名表中簡中欄非空的繁中詞，轉換前先換成私用區佔位字元（U+E000 起），
//! 讓 OpenCC 不碰它；轉換後換成簡中欄的詞。較長的詞先換，避免短詞吃掉長詞的一部分。
use ferrous_opencc::OpenCC;
use ferrous_opencc::config::BuiltinConfig;

pub struct ToZhCn {
    cc: OpenCC,
    /// (繁中詞, 簡中詞)，依繁中詞長度由長到短。
    overrides: Vec<(String, String)>,
}

/// 讀譯名表：四欄 原文、繁中、簡中、依據；`#` 開頭為註解。只取簡中欄非空的列。
pub fn glossary_overrides(tsv: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for (n, line) in tsv.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() != 4 {
            return Err(format!("譯名表第 {} 行欄數 {}，應為 4", n + 1, cols.len()));
        }
        if !cols[2].is_empty() {
            out.push((cols[1].to_string(), cols[2].to_string()));
        }
    }
    Ok(out)
}

impl ToZhCn {
    pub fn new(mut overrides: Vec<(String, String)>) -> Result<Self, String> {
        let cc = OpenCC::from_config(BuiltinConfig::Tw2sp).map_err(|e| e.to_string())?;
        overrides.sort_by(|a, b| b.0.chars().count().cmp(&a.0.chars().count()).then(a.0.cmp(&b.0)));
        overrides.dedup_by(|a, b| a.0 == b.0);
        if overrides.len() > 0x1000 {
            return Err("覆寫詞超過私用區佔位上限".into());
        }
        Ok(Self { cc, overrides })
    }

    pub fn convert(&self, zh_tw: &str) -> String {
        let mut text = zh_tw.to_string();
        let marks: Vec<char> = (0..self.overrides.len())
            .map(|i| char::from_u32(0xe000 + i as u32).unwrap())
            .collect();
        for ((tw, _), m) in self.overrides.iter().zip(&marks) {
            text = text.replace(tw.as_str(), &m.to_string());
        }
        let mut out = self.cc.convert(&text);
        for ((_, cn), m) in self.overrides.iter().zip(&marks) {
            out = out.replace(*m, cn);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taiwan_terms_become_mainland_terms_and_glossary_wins() {
        let g = "# c\nGorbachev\t戈巴契夫\t戈尔巴乔夫\tx\nMoscow\t莫斯科\t\tx\n";
        let c = ToZhCn::new(glossary_overrides(g).unwrap()).unwrap();
        assert_eq!(c.convert("戈巴契夫在莫斯科。"), "戈尔巴乔夫在莫斯科。");
        // tw2sp 的詞彙轉換：台灣用語換成大陸用語，字形轉簡體。
        assert_eq!(c.convert("軟體"), "软件");
        assert_eq!(c.convert("這是蘇聯。"), "这是苏联。");
    }

    #[test]
    fn without_override_opencc_alone_differs() {
        // 負對照：沒有覆寫時，台灣譯名只會被轉成簡體字形，不會變成大陸譯名。
        let c = ToZhCn::new(Vec::new()).unwrap();
        assert_ne!(c.convert("戈巴契夫"), "戈尔巴乔夫");
    }

    #[test]
    fn bad_glossary_row_is_rejected() {
        assert!(glossary_overrides("a\tb\tc\n").is_err());
    }
}
