#!/usr/bin/env python3
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
"""unsqx 的合成測試（不含原版資料）。用法：python3 -I test_unsqx.py（工作目錄為 tools）"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from unsqx import unsqx

# 位元 0,0 字面量 A、B；1,0 短回參照，長度位元 0,0（＝2），偏移 FEh（−2）；1,1 長回參照 w=0、長度位元組 0＝結束。
short = bytes([0, 0, 0, 1, 2, 5, 0xC4, 0x00, 0x41, 0x42, 0xFE, 0x00, 0x00, 0x00])
assert unsqx(short) == b"ABAB", unsqx(short)
# 位元 0 字面量 X；1,1 長回參照 w=FFE1h（n=5：長度 1＋2、偏移 −1）；1,1 結束。
long_ = bytes([0, 0, 0, 1, 2, 5, 0x1E, 0x00, 0x58, 0xE1, 0xFF, 0x00, 0x00, 0x00])
assert unsqx(long_) == b"XXXX", unsqx(long_)
# 負對照：n 改成 4，同一串長回參照解成別的東西（或越界失敗），不能仍是 XXXX。
bad = bytearray(long_); bad[5] = 4
try:
    assert unsqx(bytes(bad)) != b"XXXX"
except ValueError:
    pass
# 選擇子排列：sel 01 00 02 時，第一個位元 0 代表短回參照。
perm = bytes([0, 0, 1, 0, 2, 5, 0b0110, 0x00, 0x41, 0x42])
try:
    unsqx(perm)
    raise AssertionError("排列後第一個位元 0 應是短回參照，在空輸出上越界")
except ValueError:
    pass
print("unsqx selftest ok")
