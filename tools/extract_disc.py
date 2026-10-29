#!/usr/bin/env python3
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
"""從玩家自備的 KGB zip 讀出 ISO9660 光碟，解到輸出目錄並寫 manifest。

只讀根目錄（本片沒有子目錄），檔名去掉 `;1`。輸入 zip 與 ISO 的 SHA-256
必須等於 AGENTS.md 第 3 節固定的值，不符就停止。
用法（容器內）：python3 -I tools/extract_disc.py <zip> <輸出目錄>
"""
import hashlib, json, os, struct, sys, zipfile

ZIP_SHA = "a12481886606511e8a28f88a41523d0a2334032cf413b6e595020dd79e5ed3fc"
ISO_SHA = "10c1e4a28546d687bf8feb580b0460755fdbad4c05a939a31ba7b907e27d48a3"
ISO_NAME = "KGB/cd/KGB.iso"
SECTOR = 2048


def sha256(b):
    return hashlib.sha256(b).hexdigest()


def main(zip_path, out):
    raw = open(zip_path, "rb").read()
    if sha256(raw) != ZIP_SHA:
        sys.exit(f"zip SHA-256 不符：{sha256(raw)}")
    z = zipfile.ZipFile(zip_path)
    iso = z.read(ISO_NAME)
    if sha256(iso) != ISO_SHA:
        sys.exit(f"ISO SHA-256 不符：{sha256(iso)}")
    pvd = iso[16 * SECTOR:17 * SECTOR]
    if pvd[1:6] != b"CD001" or pvd[0] != 1:
        sys.exit("第 16 扇區不是 ISO9660 主卷描述")
    root = pvd[156:190]
    lba, size = struct.unpack("<I", root[2:6])[0], struct.unpack("<I", root[10:14])[0]
    buf = iso[lba * SECTOR:lba * SECTOR + size]
    cd_dir = os.path.join(out, "cd")
    os.makedirs(cd_dir, exist_ok=True)
    files, i = [], 0
    while i < len(buf):
        n = buf[i]
        if n == 0:
            i = (i // SECTOR + 1) * SECTOR
            continue
        rec = buf[i:i + n]
        i += n
        name = rec[33:33 + rec[32]]
        if name in (b"\x00", b"\x01"):
            continue
        if rec[25] & 2:
            sys.exit(f"未預期的子目錄：{name!r}")
        flba, fsize = struct.unpack("<I", rec[2:6])[0], struct.unpack("<I", rec[10:14])[0]
        fname = name.decode("ascii").split(";")[0]
        data = iso[flba * SECTOR:flba * SECTOR + fsize]
        with open(os.path.join(cd_dir, fname), "wb") as f:
            f.write(data)
        files.append({"name": fname, "lba": flba, "size": fsize, "sha256": sha256(data)})
    extra = []
    for n in z.namelist():
        if n.endswith("/") or n == ISO_NAME:
            continue
        data = z.read(n)
        base = os.path.basename(n)
        with open(os.path.join(out, base), "wb") as f:
            f.write(data)
        extra.append({"name": base, "zip_path": n, "size": len(data), "sha256": sha256(data)})
    manifest = {
        "zip_sha256": ZIP_SHA, "iso_sha256": ISO_SHA,
        "volume_id": pvd[40:72].decode("ascii").rstrip(),
        "iso_sectors": len(iso) // SECTOR,
        "cd_files": files, "zip_files": extra,
    }
    with open(os.path.join(out, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1, ensure_ascii=False)
    print(f"{len(files)} 個光碟檔、{len(extra)} 個 zip 檔 → {out}")


if __name__ == "__main__":
    main(*sys.argv[1:3])
