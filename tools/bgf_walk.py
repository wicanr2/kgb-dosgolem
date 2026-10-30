#!/usr/bin/env python3
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
# BGF 影片：沿畫格長度走完各段，統計畫格數、長度與畫格旗標（docs/re/007）。用法：bgf_walk.py 檔案:段數 ...
import struct, sys, collections
def walk(path, maxseg):
    d = open(path, 'rb').read()
    hdr = struct.unpack_from('<I', d, 0)[0]
    offs = list(struct.unpack_from('<%dI' % (hdr // 4), d, 0))
    offs[0] = hdr
    print(path, 'segments', len(offs) - 1)
    for s in range(min(maxseg, len(offs) - 1)):
        p, end = offs[s], offs[s + 1]
        n = 0; flags = collections.Counter(); sizes = []
        while p + 4 <= end:
            size = struct.unpack_from('<H', d, p)[0]
            if size < 4:
                print('  seg', s, 'bad size at', hex(p), size); break
            flags[(d[p + 2], d[p + 3])] += 1
            sizes.append(size); p += size; n += 1
        print('  seg %d frames %d end_ok %s size min/avg/max %d/%d/%d' % (s, n, p == end, min(sizes), sum(sizes)//len(sizes), max(sizes)))
        print('   flags', flags.most_common(6))
for a in sys.argv[1:]:
    path, m = a.split(':')
    walk(path, int(m))
