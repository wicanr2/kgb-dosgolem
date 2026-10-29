#!/usr/bin/env python3
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
"""由萃取的插槽範本與範本譯文產生範本譯文表（docs/spec/004 §8）。
用法：python3 -I make_templates.py <phrases.tsv> <範本譯文 id<TAB>譯文> <輸出 .zh-TW.tsv> <出處前綴>
phrases.tsv 為 kgb-extract 的輸出（第 5 欄 template）。範本鍵規則：空插槽後緊接的代名詞
him、her、it（後面不是字母）併入插槽；去掉結尾空白；以 FNV-1a 64 雜湊（同 merge_translations.py）。
檢查：每個範本都有譯文、譯文的插槽標記與範本相同且各出現一次、同一範本鍵的譯文一致。
"""
import os, re, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from merge_translations import hash_key  # noqa: E402

MARK = re.compile(r"\{[0-9a-f]{2}\}")


def canonical(template):
    t = re.sub(r"(\{[0-9a-f]{2}\})(him|her|it)(?![A-Za-z])", r"\1", template)
    return t.rstrip(" ")


def main(phrases, zh_path, out, prefix):
    rows = [l.rstrip("\n").split("\t") for l in open(phrases, encoding="utf-8")][1:]
    templates = {r[0]: canonical(r[4]) for r in rows if len(r) > 4 and r[4]}
    zh = dict(l.rstrip("\n").split("\t", 1) for l in open(zh_path, encoding="utf-8") if l.strip())
    errors, seen, lines = [], {}, []
    for i, t in templates.items():
        if i not in zh:
            errors.append(f"{i}：缺譯文（{t}）")
            continue
        marks, zmarks = sorted(MARK.findall(t)), sorted(MARK.findall(zh[i]))
        if marks != zmarks or len(set(marks)) != len(marks):
            errors.append(f"{i}：插槽標記不符 {marks} → {zmarks}")
        if t in seen:
            if seen[t] != zh[i]:
                errors.append(f"{i}：同一範本譯文不同（{seen[t]}／{zh[i]}）")
            continue
        seen[t] = zh[i]
        lines.append(f"{hash_key(t)}\t{zh[i]}\t{prefix}:{i}\n")
    extra = sorted(set(zh) - set(templates))
    if extra:
        errors.append(f"譯文多出不是範本的 id：{extra[:5]}")
    if errors:
        print("\n".join(errors))
        raise SystemExit(f"{len(errors)} 項錯誤，未寫輸出")
    with open(out, "w", encoding="utf-8") as w:
        w.write(f"# {prefix} 插槽範本譯文（docs/spec/004 §8）。三欄：key（範本的 FNV-1a 64 雜湊）、譯文（{{字碼}} 為插槽）、出處。由 tools/make_templates.py 產生。\n")
        w.writelines(lines)
    print(f"範本 {len(templates)} 個，不重複 {len(lines)} 個")


if __name__ == "__main__":
    main(*sys.argv[1:5])
