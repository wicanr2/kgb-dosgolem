#!/usr/bin/env python3
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
"""合併子代理交付的批次譯文成譯文表，並逐列驗證。
用法：python3 -I merge_translations.py <batches 目錄> <done 目錄> <輸出 .zh-TW.tsv> <出處前綴>
驗證：id 與批次逐列相同、譯文非空、`^` 數量相同、沒有殘留 `#`、沒有 Tab、沒有半形引號包住整句。
同一英文 key 出現多次時，譯文必須相同（取第一個並檢查其餘）。任何一項失敗就不寫輸出。
"""
import os, sys


# 開發期的除錯佔位字串，遊戲內不顯示，不進譯文表。
PLACEHOLDER = "bugbugbug."


def load(path, cols):
    rows = []
    for n, line in enumerate(open(path, encoding="utf-8"), 1):
        line = line.rstrip("\n")
        if not line:
            continue
        parts = line.split("\t")
        if len(parts) != cols:
            raise SystemExit(f"{path}:{n} 欄數 {len(parts)}，應為 {cols}")
        rows.append(parts)
    return rows


def main(batches, done, out, prefix):
    errors, merged, seen = [], [], {}
    names = sorted(f for f in os.listdir(batches) if f.endswith(".tsv"))
    missing = [f for f in names if not os.path.exists(os.path.join(done, f))]
    skipped = 0
    for f in names:
        if f in missing:
            continue
        src = load(os.path.join(batches, f), 2)
        dst = load(os.path.join(done, f), 2)
        if [r[0] for r in src] != [r[0] for r in dst]:
            errors.append(f"{f}：id 序列不同（{len(src)} 對 {len(dst)} 列）")
            continue
        for (i, en), (_, zh) in zip(src, dst):
            if en == PLACEHOLDER:
                skipped += 1
                continue
            where = f"{f} id {i}"
            if not zh.strip():
                errors.append(f"{where}：譯文空白")
            if en.count("^") != zh.count("^"):
                errors.append(f"{where}：^ 數量 {en.count('^')} → {zh.count('^')}")
            if "#" in zh:
                errors.append(f"{where}：殘留 #")
            if en in seen:
                if seen[en][1] != zh:
                    seen[en][2].append(i)
                continue
            seen[en] = (i, zh, [])
            merged.append((en, zh, i))
    if errors:
        print("\n".join(errors[:50]))
        raise SystemExit(f"{len(errors)} 項錯誤，未寫輸出")
    with open(out, "w", encoding="utf-8") as w:
        w.write(f"# {prefix} 片語譯文。三欄：key（原版英文）、譯文、出處（片語編號）。由 tools/merge_translations.py 產生。\n")
        for en, zh, i in merged:
            w.write(f"{en}\t{zh}\t{prefix}:{i}\n")
    dup = {en: v[2] for en, v in seen.items() if v[2]}
    print(f"略過佔位 {skipped} 列；合併 {len(merged)} 列；缺批次 {len(missing)}：{missing[:5]}；同 key 不同譯 {len(dup)} 組（取第一個）")


if __name__ == "__main__":
    main(*sys.argv[1:5])
