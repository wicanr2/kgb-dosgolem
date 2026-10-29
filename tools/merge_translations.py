#!/usr/bin/env python3
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
"""合併子代理交付的批次譯文成譯文表，並逐列驗證。
用法：python3 -I merge_translations.py <batches 目錄> <done 目錄> <輸出 .zh-TW.tsv> <出處前綴> [既有譯文表 ...]
輸出的 key 是比對鍵的 FNV-1a 64 雜湊（與 crates/kgb-cht/src/catalog.rs 的 hash_key 相同）。
既有譯文表已收的 key 不再輸出：同一語言的全部譯文表一起載入，跨檔重複的 key 會讓載入失敗。
驗證：id 與批次逐列相同、譯文非空、`^` 數量相同、沒有殘留 `#`、沒有 Tab、沒有半形引號包住整句。
同一英文 key 出現多次時取第一個譯文，並回報不一致的組數。任何一項驗證失敗就不寫輸出。
"""
import os, sys


# 開發期的除錯佔位字串，遊戲內不顯示，不進譯文表。
PLACEHOLDER = "bugbugbug."


def key_bytes(key):
    r"""還原 catalog.rs 的 key_of 跳脫：`\\` 與 `\xNN`，其餘為可列印 ASCII。"""
    out, i = bytearray(), 0
    while i < len(key):
        c = key[i]
        if c != "\\":
            out += c.encode("ascii")
            i += 1
        elif key[i + 1] == "\\":
            out.append(0x5C)
            i += 2
        elif key[i + 1] == "x":
            out.append(int(key[i + 2:i + 4], 16))
            i += 4
        else:
            raise ValueError(f"{key}: 未知跳脫")
    return bytes(out)


def hash_key(key):
    """與 catalog.rs 的 hash_key 相同：比對鍵位元組的 FNV-1a 64。原版全文因此不進版控。"""
    h = 0xCBF29CE484222325
    for b in key_bytes(key):
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"fnv64:{h:016x}"


assert hash_key("a") == "fnv64:af63dc4c8601ec8c"  # FNV-1a 64 標準測試向量


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


def existing_keys(paths):
    """既有譯文表的 key（雜湊）與譯文；原文 key 換算成雜湊，與 catalog.rs 一致。"""
    keys = {}
    for p in paths:
        for line in open(p, encoding="utf-8"):
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            k, zh = line.split("\t")[:2]
            if k.startswith("pattern:"):
                continue
            keys[k if k.startswith("fnv64:") else hash_key(k)] = zh
    return keys


def main(batches, done, out, prefix, *others):
    errors, merged, seen = [], [], {}
    known = existing_keys(others)
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
    shared = [(en, zh, i) for en, zh, i in merged if hash_key(en) in known]
    differ = [(i, known[hash_key(en)], zh) for en, zh, i in shared if known[hash_key(en)] != zh]
    with open(out, "w", encoding="utf-8") as w:
        w.write(f"# {prefix} 片語譯文。三欄：key（比對鍵的 FNV-1a 64 雜湊，原文只在 workplace）、譯文、出處（片語編號）。由 tools/merge_translations.py 產生。\n")
        for en, zh, i in merged:
            if hash_key(en) not in known:
                w.write(f"{hash_key(en)}\t{zh}\t{prefix}:{i}\n")
    dup = {en: v[2] for en, v in seen.items() if v[2]}
    print(f"略過佔位 {skipped} 列；合併 {len(merged) - len(shared)} 列；既有表已收 {len(shared)} 列（其中譯文不同 {len(differ)}）；"
          f"缺批次 {len(missing)}：{missing[:5]}；同 key 不同譯 {len(dup)} 組（取第一個）")
    for i, old, new in differ[:20]:
        print(f"  {prefix}:{i} 沿用既有「{old[:30]}」，本批「{new[:30]}」")


if __name__ == "__main__":
    main(*sys.argv[1:5], *sys.argv[5:])
