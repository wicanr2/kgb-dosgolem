// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 顯示語言（AGENTS.md §1「語言選單」）：繁中、簡中、英文原版。
//!
//! 預設依系統語系：zh-TW、zh-HK、zh-MO 為繁中，zh-CN、zh-SG 為簡中，其他語系為繁中。
//! 英文原版關閉接手與覆繪，原版照畫。

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    ZhTw,
    ZhCn,
    En,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::ZhTw, Lang::ZhCn, Lang::En];

    /// 工具列上的名稱，各用該語言自己的寫法。
    pub fn label(self) -> &'static str {
        match self {
            Lang::ZhTw => "繁中",
            Lang::ZhCn => "简中",
            Lang::En => "英文原版",
        }
    }

    /// 譯文表檔名的語言段（`text/*.<tag>.tsv`）；英文原版沒有譯文表。
    pub fn tag(self) -> Option<&'static str> {
        match self {
            Lang::ZhTw => Some("zh-TW"),
            Lang::ZhCn => Some("zh-CN"),
            Lang::En => None,
        }
    }

    /// Noto Sans CJK TTC 的 collection index：TC 為 3，SC 為 2。
    pub fn font_index(self) -> Option<u32> {
        match self {
            Lang::ZhTw => Some(3),
            Lang::ZhCn => Some(2),
            Lang::En => None,
        }
    }

    /// 命令列值：zh-TW、zh-CN、en。
    pub fn parse(s: &str) -> Option<Lang> {
        match s {
            "zh-TW" => Some(Lang::ZhTw),
            "zh-CN" => Some(Lang::ZhCn),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    /// 一個語系字串（如 `zh_CN.UTF-8`、`zh-Hant-TW`）對應的預設語言。
    pub fn from_locale(locale: &str) -> Lang {
        let l = locale.split(['.', '@']).next().unwrap_or("").to_ascii_lowercase().replace('-', "_");
        let parts: Vec<&str> = l.split('_').collect();
        if parts.first() != Some(&"zh") {
            return Lang::ZhTw;
        }
        if parts.contains(&"hans") || parts.iter().any(|p| matches!(*p, "cn" | "sg")) {
            Lang::ZhCn
        } else {
            Lang::ZhTw
        }
    }

    /// 依 POSIX 優先序 LC_ALL、LC_MESSAGES、LANG 取第一個非空值決定預設語言。
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Lang {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|k| get(k).filter(|v| !v.is_empty()))
            .map_or(Lang::ZhTw, |v| Lang::from_locale(&v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_maps_to_default_language() {
        for l in ["zh_TW.UTF-8", "zh_HK", "zh-MO", "zh_Hant_TW", "zh", "en_US.UTF-8", "ja_JP", "C", ""] {
            assert_eq!(Lang::from_locale(l), Lang::ZhTw, "{l}");
        }
        for l in ["zh_CN.UTF-8", "zh_SG", "zh-Hans", "zh_Hans_CN", "zh_CN@pinyin"] {
            assert_eq!(Lang::from_locale(l), Lang::ZhCn, "{l}");
        }
    }

    #[test]
    fn env_precedence_is_lc_all_then_lc_messages_then_lang() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        assert_eq!(Lang::from_env(env(&[("LANG", "zh_CN.UTF-8")])), Lang::ZhCn);
        assert_eq!(Lang::from_env(env(&[("LC_ALL", "zh_TW.UTF-8"), ("LANG", "zh_CN.UTF-8")])), Lang::ZhTw);
        assert_eq!(Lang::from_env(env(&[("LC_ALL", ""), ("LC_MESSAGES", "zh_SG"), ("LANG", "en_US")])), Lang::ZhCn);
        assert_eq!(Lang::from_env(env(&[])), Lang::ZhTw, "沒有語系變數時預設繁中");
    }

    #[test]
    fn english_has_no_catalog_or_font() {
        assert_eq!(Lang::En.tag(), None);
        assert_eq!(Lang::En.font_index(), None);
        assert_eq!(Lang::parse("zh-CN"), Some(Lang::ZhCn));
        assert_eq!(Lang::parse("zh_CN"), None);
    }
}
