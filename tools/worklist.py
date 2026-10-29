#!/usr/bin/env python3
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
"""檢查 docs/worklist.json：verify 為真＝仍未完成。只掃產品程式碼，略過 *_test.* 與 tests/。
用法（容器內，專案為 /project）：
  python3 -I tools/worklist.py [verify|selftest] [專案根]
"""
import json, os, sys, tempfile


def files(root, paths):
    for p in paths:
        full = os.path.join(root, p)
        if os.path.isfile(full):
            yield full
        for d, dirs, names in os.walk(full):
            dirs[:] = [x for x in dirs if x not in ("tests", "target")]
            for n in names:
                if "_test." not in n and not n.startswith("test_"):
                    yield os.path.join(d, n)


def found(root, v):
    for f in files(root, v["paths"]):
        try:
            if v["pattern"] in open(f, encoding="utf-8", errors="replace").read():
                return True
        except OSError:
            pass
    return False


def still_open(root, v):
    k = v["kind"]
    if k == "manual":
        return True
    if not any(os.path.exists(os.path.join(root, p)) for p in v["paths"]):
        raise SystemExit(f"verify 路徑不存在：{v['paths']}")
    if k == "present":
        return found(root, v)
    if k == "absent":
        return not found(root, v)
    raise SystemExit(f"未知 kind {k}")


def verify(root):
    data = json.load(open(os.path.join(root, "docs/worklist.json"), encoding="utf-8"))
    stale = 0
    for it in data["items"]:
        o = still_open(root, it["verify"])
        tag = "manual" if it["verify"]["kind"] == "manual" else ("open" if o else "STALE")
        stale += tag == "STALE"
        print(f"{tag:6} {it['layer']:8} {it['id']:16} {it['title']}")
    if stale:
        print(f"{stale} 條已不成立，請更新 docs/worklist.json")
    return 1 if stale else 0


def selftest():
    with tempfile.TemporaryDirectory() as d:
        os.makedirs(os.path.join(d, "src"))
        p = os.path.join(d, "src", "a.rs")
        open(p, "w").write("fn other() {}\n")
        open(os.path.join(d, "src", "a_test.rs"), "w").write("fn ems_call() {}\n")
        v = {"kind": "absent", "paths": ["src"], "pattern": "fn ems_call"}
        assert still_open(d, v), "測試檔不應讓 absent 判成完成"
        open(p, "a").write("fn ems_call() {}\n")
        assert not still_open(d, v), "訊號出現後 absent 應判成完成"
        v2 = {"kind": "present", "paths": ["src"], "pattern": "TODO-x"}
        assert not still_open(d, v2)
        open(p, "a").write("// TODO-x\n")
        assert still_open(d, v2)
        assert still_open(d, {"kind": "manual"})
        try:
            still_open(d, {"kind": "absent", "paths": ["nope"], "pattern": "x"})
            raise AssertionError("不存在的路徑應失敗")
        except SystemExit:
            pass
    print("selftest ok")
    return 0


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "verify"
    root = sys.argv[2] if len(sys.argv) > 2 else os.getcwd()
    sys.exit(selftest() if cmd == "selftest" else verify(root))
