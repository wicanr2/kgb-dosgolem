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
#   tools/rust.sh kgb <輸出名> <步數> [--shots N] [--input /inputs/<檔>] [--trace-phrases]
#                                  接手文字 routine，輸出事件與中文覆繪截圖；輸入腳本放 workplace/inputs
#   tools/rust.sh play [--lang zh-TW|zh-CN|en] [--cycles N]   在本機 X11 桌面開視窗玩；預設語言依主機語系
#   tools/rust.sh gui-check <輸出名> <秒數> [--lang ...]       Xvfb 無頭跑視窗，擷取畫面為 PNG
#                                  原版 workplace/disc/cd 唯讀共用，每次執行只建可寫暫存層，
#                                  在自有核心上跑 KGB 啟動器，輸出到 workplace/runs/<輸出名>
#
# 映像預設為共用基底 dosgolem-rust-sc:1.99.1-r1（不修改它），可用 DGRS_RUST_IMAGE 覆寫。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${DGRS_RUST_IMAGE:-dosgolem-rust-sc:1.99.1-r1}"
# GUI 執行與無頭驗證：既有映像（Xvfb、xdotool、Mesa 25 lavapipe），不修改它。
GUI_IMAGE="${DGRS_GUI_IMAGE:-sha256:a5b26f4bd035234bbcaab0e12c3081bb70c4833f1b1c4983d06fbcea98eb7ad8}"
WP="$ROOT/workplace"
FG_ROOT="${FG_ROOT:-$(cd "$ROOT/../ferrogolem" 2>/dev/null && pwd)}"
test -f "$FG_ROOT/crates/ferrogolem-core/Cargo.toml" || { echo "找不到 FerroGolem（$FG_ROOT）" >&2; exit 1; }
mkdir -p "$WP/cargo" "$WP/target" "$WP/runs"

docker image inspect "$IMAGE" >/dev/null 2>&1 || { echo "找不到映像 $IMAGE" >&2; exit 1; }

# 新建暫存層時放入原版設定檔，並把第 5 位元組（[061d] 字幕旗標，docs/re/005）設為 FFh，
# 說明影片預設顯示字幕供中文覆繪。已有的設定檔不動。
new_cfg() {
  test -f "$1/CSP.CFG" && return 0
  cp "$WP/disc/CSP.CFG" "$1/"
  printf '\377' | dd of="$1/CSP.CFG" bs=1 seek=5 conv=notrunc status=none
}

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
    mkdir -p "$run/scratch/KGB" "$WP/inputs"
    new_cfg "$run/scratch/KGB"
    tsv=(); for f in "$ROOT"/text/*.zh-TW.tsv; do test -f "$f" && tsv+=(--tsv "/text/$(basename "$f")"); done
    timeout 3600 docker run --rm -u "$(id -u):$(id -g)" --name "dgrs-kgb-$name" \
      --memory 3g --cpus 1 --pids-limit 64 --network none \
      --log-opt max-size=10m --log-opt max-file=3 \
      -v "$WP/target/release:/tool:ro" -v "$WP/disc/cd:/game:ro" -v "$ROOT/text:/text:ro" \
      -v "$font_dir:/fonts:ro" -v "$WP/inputs:/inputs:ro" -v "$run:/run" "$IMAGE" \
      /tool/kgb-run --root /game --scratch /run/scratch --out /run/out --steps "$steps" \
      --font /fonts/NotoSansCJK-Medium.ttc --font-index 3 "${tsv[@]}" --labels /text/sprite-labels.tsv "${extra[@]}"
    ;;
  play)
    # 在本機 X11 桌面開視窗玩。存檔留在 workplace/play/scratch。
    [[ "${DISPLAY:-}" =~ ^:[0-9]+(\.[0-9]+)?$ ]] || { echo "請在已登入 X11／XWayland 桌面的終端執行（需要 DISPLAY）" >&2; exit 2; }
    test -d /tmp/.X11-unix || { echo "找不到 /tmp/.X11-unix" >&2; exit 1; }
    test -x "$WP/target/release/kgb-play" || { echo "先跑 tools/rust.sh build" >&2; exit 1; }
    font_dir=/usr/share/fonts/opentype/noto
    test -f "$font_dir/NotoSansCJK-Medium.ttc" || { echo "找不到 Noto Sans CJK" >&2; exit 1; }
    mkdir -p "$WP/play/scratch/KGB"
    new_cfg "$WP/play/scratch/KGB"
    gpu=(); if test -c /dev/dri/renderD128; then gpu=(--device /dev/dri/renderD128 --group-add "$(stat -c %g /dev/dri/renderD128)"); fi
    auth=(); xa="${XAUTHORITY:-$HOME/.Xauthority}"; if test -f "$xa"; then auth=(-v "$xa:/tmp/xauth:ro" -e XAUTHORITY=/tmp/xauth); fi
    # 語系變數照主機傳入，kgb-play 依此選預設語言（未設定的變數不會傳）。
    docker run --rm --init -u "$(id -u):$(id -g)" --name "dgrs-play-$$" \
      --memory 4g --cpus 4 --pids-limit 256 --network none --read-only --tmpfs /tmp:rw,exec,size=256m \
      --cap-drop ALL --security-opt no-new-privileges \
      --log-opt max-size=10m --log-opt max-file=3 \
      -e HOME=/tmp -e DISPLAY -e WGPU_BACKEND=vulkan -e LANG -e LC_ALL -e LC_MESSAGES "${gpu[@]}" "${auth[@]}" \
      -v /tmp/.X11-unix:/tmp/.X11-unix:ro -v "$WP/target/release:/tool:ro" -v "$WP/disc/cd:/game:ro" \
      -v "$ROOT/text:/text:ro" -v "$font_dir:/fonts:ro" -v "$WP/play/scratch:/save" "$GUI_IMAGE" \
      /tool/kgb-play --root /game --scratch /save --text /text --font /fonts/NotoSansCJK-Medium.ttc "$@"
    ;;
  gui-check)
    # 無頭驗證視窗：Xvfb＋lavapipe，跑指定秒數後擷取畫面為 PNG。
    name="${1:?輸出名}"; secs="${2:?秒數}"; shift 2
    test -x "$WP/target/release/kgb-play" || { echo "先跑 tools/rust.sh build" >&2; exit 1; }
    run="$WP/runs/$name"; test ! -e "$run" || { echo "$run 已存在" >&2; exit 1; }
    mkdir -p "$run/scratch/KGB" "$run/fb"; new_cfg "$run/scratch/KGB"
    timeout $((secs + 60)) docker run --rm --init -u "$(id -u):$(id -g)" --name "dgrs-gui-$name" \
      --memory 3g --cpus 3 --pids-limit 256 --network none --log-opt max-size=10m --log-opt max-file=3 \
      -e HOME=/tmp -e WGPU_BACKEND=vulkan -v "$WP/target/release:/tool:ro" -v "$WP/disc/cd:/game:ro" \
      -v "$ROOT/text:/text:ro" -v /usr/share/fonts/opentype/noto:/fonts:ro -v "$run:/run" "$GUI_IMAGE" \
      sh -c 'Xvfb :99 -screen 0 1000x700x24 -fbdir /run/fb >/run/xvfb.log 2>&1 & sleep 2; DISPLAY=:99 "$@" >/run/play.log 2>&1 & sleep '"$secs"'; cp /run/fb/Xvfb_screen0 /run/shot.xwd' \
      sh /tool/kgb-play --root /game --scratch /run/scratch --text /text --font /fonts/NotoSansCJK-Medium.ttc "$@"
    timeout 60 docker run --rm --network none --memory 512m --pids-limit 32 --log-opt max-size=10m --log-opt max-file=3 \
      -u "$(id -u):$(id -g)" -v "$run:/d" -v "$FG_ROOT/tools/py:/py:ro" --entrypoint python3 "$IMAGE" -I /py/xwd2png.py /d/shot.xwd /d/shot.png
    echo "$run/shot.png"
    ;;
  *) echo "未知子命令 $cmd" >&2; exit 2 ;;
esac
