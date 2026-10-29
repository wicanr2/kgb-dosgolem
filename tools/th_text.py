#!/usr/bin/env python3
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
"""說明影片字幕文字（TH?*.SQX）切成執行期的字幕句（docs/re/005-help-subtitles.md）。
用法：python3 -I th_text.py <TH*.SQX>...      輸出 TSV：id<TAB>句子（比對鍵寫法同 catalog.rs key_of）
id＝檔名主幹:條目:句序。檔案以 SQX 解壓後，開頭是 16 位元小端偏移表（值減 100h 為位置，第一個也是表長），
每個條目是 0 結尾的字串；THAT、THAI 沒有偏移表，整檔是一個條目。切句依 188b:2c57–2cba：略過開頭空白；逐字複製，遇 . ; : ? ! 時
看下一字，下一字也是這些字元、^ 或 0 就繼續，否則結束本句；^ 不複製；讀到 0 為最後一句。
比對鍵再依 docs/spec/004 §4 正規化（去尾端 0 與空白）。
"""
import os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from unsqx import unsqx  # noqa: E402

STOP = b".;:?!"
# 偏移表的值比實際位置多 100h（188b:2b7a：取表值後 sub si,100h）。
BASE = 0x100


def key_of(b):
    out = []
    for c in b:
        if c == 0x5C:
            out.append("\\\\")
        elif 0x20 <= c <= 0x7E:
            out.append(chr(c))
        else:
            out.append(f"\\x{c:02x}")
    return "".join(out)


def sentences(s):
    """依 2c57–2cba 把一個條目（不含結尾 0）切成句子。"""
    s = bytes(s) + b"\0"
    i, out = 0, []
    while True:
        while s[i] == 0x20:
            i += 1
        buf = bytearray()
        while True:
            c = s[i]
            i += 1
            if c != 0x5E:
                buf.append(c)
                if c == 0:
                    out.append(bytes(buf))
                    return out
                if c not in STOP:
                    continue
            n = s[i]
            if n == 0 or n == 0x5E or n in STOP:
                continue
            break
        out.append(bytes(buf))


def normalize(b):
    b = bytearray(b)
    while b and b[-1] in (0, 0x20):
        b.pop()
    return bytes(c if c else 0x20 for c in b)


def entries(data):
    """有偏移表的檔逐條目；沒有表（THAT、THAI：第一個字組超出檔長）時整檔是一個條目。"""
    first = int.from_bytes(data[0:2], "little") - BASE
    if not 0 < first < len(data) or first % 2:
        end = data.find(0)
        yield 0, data[: end if end >= 0 else len(data)]
        return
    offs = [int.from_bytes(data[k:k + 2], "little") - BASE for k in range(0, first, 2)]
    for n, o in enumerate(offs):
        end = data.find(0, o)
        yield n, data[o: end if end >= 0 else len(data)]


def main(paths):
    for p in paths:
        stem = os.path.basename(p).split(".")[0]
        data = unsqx(open(p, "rb").read())
        for n, e in entries(data):
            for k, sent in enumerate(sentences(e)):
                key = normalize(sent)
                if key:
                    print(f"{stem}:{n}:{k}\t{key_of(key)}")


if __name__ == "__main__":
    main(sys.argv[1:])
