#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
# KGB 中文化的建置與執行一律在 Docker。主機只呼叫本腳本。
# 依賴 ~/cht/ferrogolem（FG_ROOT 可覆寫），唯讀掛在容器的 /ferrogolem。
# 通用探針改用 FerroGolem 的 tools/rust.sh probe。
#
#   tools/rust.sh fetch            產生或更新 Cargo.lock 並抓依賴（唯一開網路的一步）
#   tools/rust.sh build            離線建置 release
#   tools/rust.sh test             離線跑測試
#   tools/rust.sh cargo <參數...>  離線執行任意 cargo 子命令（專案唯讀）
#   tools/rust.sh kgb <輸出名> <步數> [--shots N]   接手文字 routine，輸出事件與中文覆繪截圖
#                                  原版 workplace/disc/cd 唯讀共用，每次執行只建可寫暫存層，
#                                  在自有核心上跑 KGB 啟動器，輸出到 workplace/runs/<輸出名>
#
# 映像預設為共用基底 dosgolem-rust-sc:1.99.1-r1（不修改它），可用 DGRS_RUST_IMAGE 覆寫。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${DGRS_RUST_IMAGE:-dosgolem-rust-sc:1.99.1-r1}"
WP="$ROOT/workplace"
FG_ROOT="${FG_ROOT:-$(cd "$ROOT/../ferrogolem" 2>/dev/null && pwd)}"
test -f "$FG_ROOT/crates/ferrogolem-core/Cargo.toml" || { echo "找不到 FerroGolem（$FG_ROOT）" >&2; exit 1; }
mkdir -p "$WP/cargo" "$WP/target" "$WP/runs"

docker image inspect "$IMAGE" >/dev/null 2>&1 || { echo "找不到映像 $IMAGE" >&2; exit 1; }

base=(docker run --rm -u "$(id -u):$(id -g)"
  --memory 6g --cpus 4 --pids-limit 512
  --log-opt max-size=10m --log-opt max-file=3
  -e CARGO_HOME=/cache -e CARGO_TARGET_DIR=/target -e HOME=/tmp
  -v "$WP/cargo:/cache" -v "$WP/target:/target" -v "$FG_ROOT:/ferrogolem:ro" -w /project)

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
  kgb)
    # 接手文字 routine 的無頭執行：輸出文字事件與 3 倍中文覆繪截圖。
    name="${1:?輸出名}"; steps="${2:?步數}"; shift 2; extra=("$@")
    test -f "$WP/disc/manifest.json" || { echo "先解出 workplace/disc" >&2; exit 1; }
    test -x "$WP/target/release/kgb-run" || { echo "先跑 tools/rust.sh build" >&2; exit 1; }
    font_dir=/usr/share/fonts/opentype/noto
    test -f "$font_dir/NotoSansCJK-Medium.ttc" || { echo "找不到 $font_dir/NotoSansCJK-Medium.ttc" >&2; exit 1; }
    mkdir -p "$ROOT/text"
    run="$WP/runs/$name"
    test ! -e "$run" || { echo "$run 已存在，換一個輸出名" >&2; exit 1; }
    mkdir -p "$run/scratch/KGB"
    cp "$WP/disc/CSP.CFG" "$run/scratch/KGB/"
    tsv=(); for f in "$ROOT"/text/*.zh-TW.tsv; do test -f "$f" && tsv+=(--tsv "/text/$(basename "$f")"); done
    timeout 3600 docker run --rm -u "$(id -u):$(id -g)" --name "dgrs-kgb-$name" \
      --memory 3g --cpus 1 --pids-limit 64 --network none \
      --log-opt max-size=10m --log-opt max-file=3 \
      -v "$WP/target/release:/tool:ro" -v "$WP/disc/cd:/game:ro" -v "$ROOT/text:/text:ro" \
      -v "$font_dir:/fonts:ro" -v "$run:/run" "$IMAGE" \
      /tool/kgb-run --root /game --scratch /run/scratch --out /run/out --steps "$steps" \
      --font /fonts/NotoSansCJK-Medium.ttc --font-index 3 "${tsv[@]}" "${extra[@]}"
    ;;
  *) echo "未知子命令 $cmd" >&2; exit 2 ;;
esac
