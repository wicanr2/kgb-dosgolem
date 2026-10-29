#!/usr/bin/env python3
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
"""Cryo SQX 解壓。依 CSPPRG.EXE 解壓後 load segment 的 `79fc`–`7abd` 取證（docs/re/003-sqx-format.md）。

格式（位元組位置從 0 起）：
- 0–1：遊戲先單獨讀這 2 位元組（用途見 docs/re/003），解壓不使用。
- 2–4：三個選擇子，各自從 `cs:79f5` 表選出操作（0 字面量、1 短回參照、2 長回參照）：
  第一個位元為 0 → 操作[sel0]；否則第二個位元為 0 → 操作[sel1]；否則 → 操作[sel2]。
- 5：n，長回參照字組中長度所佔的低位元數。
- 6 起：資料。位元佇列為 16 位元小端字組、低位元先出，讀完再取下一個字組。
- 字面量：複製 1 位元組。
- 短回參照：再取 2 位元（先取的是高位）得 c，讀 1 位元組 b，從 目前位置＋(b−256) 複製 c＋2 位元組。
- 長回參照：讀小端字組 w，長度 c＝w & (2^n−1)，偏移＝(w >> n) 以 1 補滿高位（負數）；
  c 為 0 時再讀 1 位元組當 c，仍為 0 即結束；複製 c＋2 位元組。
用法：python3 -I unsqx.py <in.sqx> <out.bin>
"""
import sys

OPS = ("literal", "short", "long")


def unsqx(data):
    sel = data[2:5]
    n = data[5]
    if any(s > 2 for s in sel) or not 1 <= n <= 15:
        raise ValueError(f"非預期的 SQX 參數：選擇子 {sel.hex()}、n={n}")
    ops = [OPS[s] for s in sel]
    pos, queue, out = 6, 1, bytearray()

    def bit():
        nonlocal pos, queue
        if queue == 1:
            queue = (data[pos] | data[pos + 1] << 8) | 0x10000
            pos += 2
        b = queue & 1
        queue >>= 1
        return b

    def byte():
        nonlocal pos
        pos += 1
        return data[pos - 1]

    def copy(off, count):
        start = len(out) + off
        if start < 0:
            raise ValueError(f"回參照越界：位置 {len(out)}、偏移 {off}")
        for i in range(count):
            out.append(out[start + i])

    low_mask = (1 << n) - 1
    while True:
        op = ops[0] if bit() == 0 else (ops[1] if bit() == 0 else ops[2])
        if op == "literal":
            out.append(byte())
        elif op == "short":
            c = bit() << 1
            c |= bit()
            off = byte() - 256
            copy(off, c + 2)
        else:
            w = byte() | byte() << 8
            c = w & low_mask
            off = (w >> n) - (1 << (16 - n))
            if c == 0:
                c = byte()
                if c == 0:
                    return bytes(out)
            copy(off, c + 2)


if __name__ == "__main__":
    src, dst = sys.argv[1:3]
    out = unsqx(open(src, "rb").read())
    open(dst, "wb").write(out)
    print(len(out))
