#!/usr/bin/env python3
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
"""把 320x200 色號檔加 256x3 調色盤（6 位元或 8 位元）轉成 PNG。只用標準庫。
用法：python3 -I raw2png.py <色號檔> <調色盤檔> <輸出.png> [倍率]"""
import struct, sys, zlib

def main(idx_path, pal_path, out, scale="2"):
    idx, pal, k = open(idx_path, "rb").read(), open(pal_path, "rb").read(), int(scale)
    w, h = 320, len(idx) // 320
    six = max(pal) < 64
    rgb = [(((v << 2) | (v >> 4)) if six else v) for v in pal]
    rows = []
    for y in range(h):
        line = bytearray()
        for x in range(w):
            c = idx[y * w + x] * 3
            line += bytes(rgb[c:c + 3]) * k
        rows.extend([b"\0" + bytes(line)] * k)
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w * k, h * k, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")
    open(out, "wb").write(png)

if __name__ == "__main__":
    main(*sys.argv[1:])
