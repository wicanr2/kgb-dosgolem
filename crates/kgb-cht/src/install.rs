// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 發行包的啟動準備：找包內的譯文、字型與原版，第一次啟動核對原版雜湊，
//! 在使用者資料目錄建立存檔層與開了字幕的設定檔（~/cht/AGENTS_DOSGOLEM_CHT.md §15.1）。
use std::path::{Path, PathBuf};

/// 原版主程式 `CSPPRG.EXE`（光碟根目錄）的 SHA-256（AGENTS.md §3、workplace/disc/manifest.json）。
pub const CSPPRG_SHA256: &str = "d2fb547ec2516328915a320fcc2ea2330457533a8ae222e1c9821d56f5aac47c";
/// 原版硬碟目錄的 `CSP.CFG`（6 bytes）的 SHA-256。
pub const CSP_CFG_SHA256: &str = "263d99957c7b574c63a265b32da7fff8b8ad831828946bfbff650d7074dd9198";

/// 包內的路徑。
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub text: PathBuf,
    pub font: PathBuf,
    /// 原版光碟內容（`CSPPRG.EXE` 所在）。
    pub cd: PathBuf,
    /// 原版的 `CSP.CFG`。
    pub cfg: PathBuf,
}

/// 依執行檔位置找包的根目錄：執行檔旁、上一層、macOS `.app` 的 `../Resources`、AppImage 的 `../share/kgb-cht`。
/// 第一個含 `text/` 的目錄即為根。
pub fn find_base(exe_dir: &Path) -> Option<PathBuf> {
    let parent = exe_dir.parent().unwrap_or(exe_dir);
    [exe_dir.to_path_buf(), parent.to_path_buf(), parent.join("Resources"), parent.join("share").join("kgb-cht")]
        .into_iter()
        .find(|d| d.join("text").is_dir())
}

pub fn layout(base: &Path) -> Layout {
    Layout {
        text: base.join("text"),
        font: base.join("font").join("NotoSansCJK-Medium.ttc"),
        cd: base.join("original").join("cd"),
        cfg: base.join("original").join("CSP.CFG"),
    }
}

/// 使用者資料目錄：Windows `%APPDATA%`、macOS `~/Library/Application Support`、其他 `$XDG_DATA_HOME` 或 `~/.local/share`；
/// 再接 `kgb-cht`。`env` 讀環境變數（測試用注入）。
pub fn data_dir(os: &str, env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let base = match os {
        "windows" => PathBuf::from(env("APPDATA")?),
        "macos" => PathBuf::from(env("HOME")?).join("Library").join("Application Support"),
        _ => match env("XDG_DATA_HOME").filter(|s| !s.is_empty()) {
            Some(x) => PathBuf::from(x),
            None => PathBuf::from(env("HOME")?).join(".local").join("share"),
        },
    };
    Some(base.join("kgb-cht"))
}

/// 核對原版並建立存檔層；回傳（原版唯讀根、存檔層）。存檔層內的 `KGB\CSP.CFG` 只在不存在時由原版複製，
/// 並把第 5 位元組（字幕旗標 `[061d]`，docs/re/005）設成 FFh；之後玩家在選項裡的修改會保留。
pub fn prepare(l: &Layout, data: &Path) -> Result<(PathBuf, PathBuf), String> {
    let exe = l.cd.join("CSPPRG.EXE");
    let bytes = std::fs::read(&exe).map_err(|_| {
        format!("找不到原版主程式 {}。請把 KGB 光碟（卷名 CONSPIRACY）的全部檔案放到 {}。", exe.display(), l.cd.display())
    })?;
    if sha256_hex(&bytes) != CSPPRG_SHA256 {
        return Err(format!("{} 不是支援的版本（SHA-256 不符）。本中文化只支援 KGB CD 版。", exe.display()));
    }
    let save = data.join("save");
    let kgb = save.join("KGB");
    std::fs::create_dir_all(&kgb).map_err(|e| format!("無法建立存檔目錄 {}：{e}", kgb.display()))?;
    let cfg = kgb.join("CSP.CFG");
    if !cfg.exists() {
        let mut c = std::fs::read(&l.cfg).map_err(|_| format!("找不到原版設定檔 {}", l.cfg.display()))?;
        if sha256_hex(&c) != CSP_CFG_SHA256 || c.len() != 6 {
            return Err(format!("{} 與原版不符（SHA-256）", l.cfg.display()));
        }
        c[5] = 0xff;
        std::fs::write(&cfg, c).map_err(|e| format!("無法寫入 {}：{e}", cfg.display()))?;
    }
    Ok((l.cd.clone(), save))
}

/// SHA-256（FIPS 180-4），只供核對原版檔。
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01,
        0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
        0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
        0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08,
        0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, b) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            (hh, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_standard_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(sha256_hex(long), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
    }

    #[test]
    fn data_dir_per_os() {
        let env = |k: &str| match k {
            "HOME" => Some("/home/u".into()),
            "APPDATA" => Some("C:\\Users\\u\\AppData\\Roaming".into()),
            _ => None,
        };
        assert_eq!(data_dir("linux", env), Some(PathBuf::from("/home/u/.local/share/kgb-cht")));
        assert_eq!(data_dir("macos", env), Some(PathBuf::from("/home/u/Library/Application Support/kgb-cht")));
        assert!(data_dir("windows", env).unwrap().ends_with("kgb-cht"));
    }

    #[test]
    fn prepare_rejects_missing_and_wrong_original() {
        let dir = std::env::temp_dir().join(format!("kgb-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let l = layout(&dir.join("pkg"));
        assert!(prepare(&l, &dir.join("data")).unwrap_err().contains("找不到原版主程式"));
        std::fs::create_dir_all(&l.cd).unwrap();
        std::fs::write(l.cd.join("CSPPRG.EXE"), b"not kgb").unwrap();
        assert!(prepare(&l, &dir.join("data")).unwrap_err().contains("SHA-256 不符"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn find_base_prefers_exe_dir_then_resources() {
        let dir = std::env::temp_dir().join(format!("kgb-base-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let app = dir.join("KGB.app").join("Contents");
        std::fs::create_dir_all(app.join("MacOS")).unwrap();
        std::fs::create_dir_all(app.join("Resources").join("text")).unwrap();
        assert_eq!(find_base(&app.join("MacOS")), Some(app.join("Resources")));
        std::fs::create_dir_all(app.join("MacOS").join("text")).unwrap();
        assert_eq!(find_base(&app.join("MacOS")), Some(app.join("MacOS")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
