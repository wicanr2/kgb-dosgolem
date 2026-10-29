// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 由繁中譯文表產生簡中譯文表（AGENTS.md §1「兩個中文版本」，text/TRANSLATING.md）。
//!
//! 對 `<text 目錄>` 下每個 `*.zh-TW.tsv`，譯文欄與註解行經 `zhcn::ToZhCn` 轉換，
//! key 與出處欄原樣保留，寫成同名的 `*.zh-CN.tsv`。專名以 `glossary.tsv` 的簡中欄為準。
//!
//! kgb-zhcn <text 目錄>           產生
//! kgb-zhcn --check <text 目錄>   重產後與現有檔比對，不一致即失敗
use kgb_cht::zhcn::{glossary_overrides, ToZhCn};
use std::{fs, path::Path, process::ExitCode};

fn convert_catalog(cc: &ToZhCn, name: &str, tsv: &str) -> Result<String, String> {
    let mut out = format!("# 由 kgb-zhcn 從 {name} 產生，不要手改；改繁中或譯名表後重產。\n");
    for (n, line) in tsv.lines().enumerate() {
        if line.is_empty() {
            out.push('\n');
            continue;
        }
        if line.starts_with('#') {
            out.push_str(&cc.convert(line));
            out.push('\n');
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() != 3 {
            return Err(format!("{name} 第 {} 行欄數 {}，應為 3", n + 1, cols.len()));
        }
        out.push_str(&format!("{}\t{}\t{}\n", cols[0], cc.convert(cols[1]), cols[2]));
    }
    Ok(out)
}

fn main_inner() -> Result<bool, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (check, dir) = match args.as_slice() {
        [d] => (false, d),
        [f, d] if f == "--check" => (true, d),
        _ => return Err("用法：kgb-zhcn [--check] <text 目錄>".into()),
    };
    let dir = Path::new(dir);
    let glossary = fs::read_to_string(dir.join("glossary.tsv")).map_err(|e| format!("glossary.tsv：{e}"))?;
    let cc = ToZhCn::new(glossary_overrides(&glossary)?)?;
    let mut names: Vec<String> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".zh-TW.tsv"))
        .collect();
    names.sort();
    let mut clean = true;
    for name in &names {
        let tsv = fs::read_to_string(dir.join(name)).map_err(|e| format!("{name}：{e}"))?;
        let cn = convert_catalog(&cc, name, &tsv)?;
        let target = dir.join(name.replace(".zh-TW.tsv", ".zh-CN.tsv"));
        if check {
            let old = fs::read_to_string(&target).unwrap_or_default();
            if old != cn {
                println!("不一致：{}", target.display());
                clean = false;
            }
        } else {
            fs::write(&target, cn).map_err(|e| format!("{}：{e}", target.display()))?;
            println!("寫入 {}", target.display());
        }
    }
    Ok(clean)
}

fn main() -> ExitCode {
    match main_inner() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("kgb-zhcn: {e}");
            ExitCode::FAILURE
        }
    }
}
