#!/usr/bin/env python3
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
# BGF 影片：印出指定段、指定畫格的前 48 位元組（docs/re/007）。用法：bgf_frame.py 檔案:段:畫格,畫格 ...
import struct, sys
def frames(path, seg, want):
    d = open(path, 'rb').read()
    hdr = struct.unpack_from('<I', d, 0)[0]
    offs = list(struct.unpack_from('<%dI' % (hdr // 4), d, 0)); offs[0] = hdr
    p = offs[seg]; i = 0
    while i < want[-1] + 1:
        size = struct.unpack_from('<H', d, p)[0]
        if i in want:
            print(path, 'seg', seg, 'frame', i, 'off', hex(p), 'size', size)
            print('  ', d[p:p+48].hex(' '))
        p += size; i += 1
for a in sys.argv[1:]:
    path, seg, fr = a.split(':')
    frames(path, int(seg), [int(x) for x in fr.split(',')])
