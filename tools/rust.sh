#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
# Rust 建置與探針一律在 Docker。主機只呼叫本腳本。
#
#   tools/rust.sh fetch            產生或更新 Cargo.lock 並抓依賴（唯一開網路的一步）
#   tools/rust.sh build            離線建置 release
#   tools/rust.sh test             離線跑測試
#   tools/rust.sh cargo <參數...>  離線執行任意 cargo 子命令（專案唯讀）
#   tools/rust.sh probe <輸出名> <步數>
#                                  以 workplace/disc 為來源建立可寫 DOS 根目錄，
#                                  在自有核心上跑 KGB 啟動器，輸出到 workplace/runs/<輸出名>
#
# 映像預設為共用基底 dosgolem-rust-sc:1.99.1-r1（不修改它），可用 DGRS_RUST_IMAGE 覆寫。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${DGRS_RUST_IMAGE:-dosgolem-rust-sc:1.99.1-r1}"
WP="$ROOT/workplace"
mkdir -p "$WP/cargo" "$WP/target" "$WP/runs"

docker image inspect "$IMAGE" >/dev/null 2>&1 || { echo "找不到映像 $IMAGE" >&2; exit 1; }

base=(docker run --rm -u "$(id -u):$(id -g)"
  --memory 6g --cpus 4 --pids-limit 512
  --log-opt max-size=10m --log-opt max-file=3
  -e CARGO_HOME=/cache -e CARGO_TARGET_DIR=/target -e HOME=/tmp
  -v "$WP/cargo:/cache" -v "$WP/target:/target" -w /project)

cmd="${1:?用法見檔頭}"; shift || true
case "$cmd" in
  fetch)
    timeout 900 "${base[@]}" --name dgrs-fetch --network bridge \
      -v "$ROOT:/project" "$IMAGE" sh -c 'cargo generate-lockfile && cargo fetch --locked'
    ;;
  build)
    timeout 1800 "${base[@]}" --name dgrs-build --network none \
      -v "$ROOT:/project:ro" "$IMAGE" cargo build --release --locked --offline
    ;;
  test)
    timeout 1800 "${base[@]}" --name dgrs-test --network none \
      -v "$ROOT:/project:ro" "$IMAGE" cargo test --release --locked --offline "$@"
    ;;
  cargo)
    timeout 1800 "${base[@]}" --name dgrs-cargo --network none \
      -v "$ROOT:/project:ro" "$IMAGE" cargo "$@" --locked --offline
    ;;
  probe)
    name="${1:?輸出名}"; steps="${2:?步數}"
    test -f "$WP/disc/manifest.json" || { echo "先跑 tools/extract_disc.py 解出 workplace/disc" >&2; exit 1; }
    test -x "$WP/target/release/dgrs-probe" || { echo "先跑 tools/rust.sh build" >&2; exit 1; }
    run="$WP/runs/$name"
    test ! -e "$run" || { echo "$run 已存在，換一個輸出名" >&2; exit 1; }
    mkdir -p "$run/root/KGB"
    # 可寫副本：原版 workplace/disc 不被寫入。CONSP.BAT 指定寫入路徑 C:\KGB\。
    cp --reflink=auto "$WP/disc/cd/"* "$run/root/"
    cp "$WP/disc/CSP.CFG" "$run/root/KGB/"
    timeout 1800 docker run --rm -u "$(id -u):$(id -g)" --name "dgrs-probe-$name" \
      --memory 2g --cpus 1 --pids-limit 64 --network none \
      --log-opt max-size=10m --log-opt max-file=3 \
      -v "$WP/target/release:/tool:ro" -v "$run:/run" "$IMAGE" \
      /tool/dgrs-probe --root /run/root --exe CSPPRG.EXE \
      --tail ' AMR ADP220 SBP2227 EMS 386 SAF WRIC:\KGB\' \
      --steps "$steps" --out /run/out
    ;;
  *) echo "未知子命令 $cmd" >&2; exit 2 ;;
esac
