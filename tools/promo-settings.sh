#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
# 推廣片補拍：視窗版 kgb-play 在 Xvfb＋lavapipe 執行，先連點略過開場與名單、停在第一章辦公室
# （約 30 秒；開場影片畫面仍有點陣網格，issue #7，所以不拿它當背景）。接著以 xdotool 操作工具列
# 「設定」：速度滑桿拉到最高（100M）、快轉倍率拉到 8×、關閉設定後開快轉，游標掃過別洛夫與書桌。
# 期間每 0.1 秒複製一次 Xvfb 畫面緩衝（XWD），再在 kgb-cht-video 映像內依實際經過時間編成
# workplace/promo/<語言>/settings.mp4（960×660）。座標對應 FerroGolem 前端的工具列與設定視窗預設位置
# （docs/spec/006）；遊戲畫面為 3 倍，從視窗 y＝24 起。
#
#   tools/promo-settings.sh <zh-TW|zh-CN>
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WP="$ROOT/workplace"
lang="${1:?語言 zh-TW 或 zh-CN}"
case "$lang" in zh-TW) locale=zh_TW.UTF-8 ;; zh-CN) locale=zh_CN.UTF-8 ;; *) echo "語言只接受 zh-TW、zh-CN" >&2; exit 1 ;; esac
GUI_IMAGE="$(grep -o 'sha256:[0-9a-f]*' "$ROOT/tools/rust.sh" | head -1)"
test -x "$WP/target/release/kgb-play" || { echo "先跑 tools/rust.sh build" >&2; exit 1; }
run="$WP/runs/promo-settings-$lang"
rm -rf "${run:?}"
mkdir -p "$run/scratch/KGB" "$run/fb" "$run/cap" "$WP/promo/$lang"
cp "$WP/disc/CSP.CFG" "$run/scratch/KGB/"
printf '\377' | dd of="$run/scratch/KGB/CSP.CFG" bs=1 seek=5 conv=notrunc status=none  # 開字幕（同 rust.sh new_cfg）
lim=(--rm -u "$(id -u):$(id -g)" --network none --log-opt max-size=10m --log-opt max-file=3)
timeout 200 docker run "${lim[@]}" --init --name "dgrs-promo-set-$lang" --memory 3g --cpus 6 --pids-limit 256 \
  -e HOME=/tmp -e WGPU_BACKEND=vulkan -e LANG="$locale" \
  -v "$WP/target/release:/tool:ro" -v "$WP/disc/cd:/game:ro" -v "$ROOT/text:/text:ro" \
  -v /usr/share/fonts/opentype/noto:/fonts:ro -v "$run:/run" "$GUI_IMAGE" sh -c '
    Xvfb :99 -screen 0 1000x700x24 -fbdir /run/fb >/run/xvfb.log 2>&1 & sleep 2
    export DISPLAY=:99
    /tool/kgb-play --root /game --scratch /run/scratch --text /text --font /fonts/NotoSansCJK-Medium.ttc >/run/play.log 2>&1 &
    sleep 6; t=6
    while [ $t -lt 26 ]; do xdotool mousemove 480 300 mousedown 1 sleep 0.15 mouseup 1; sleep 1.8; t=$((t+2)); done
    xdotool mousemove 600 640; sleep 4
    ( date +%s.%N > /run/cap/start; i=0
      while [ $i -lt 200 ]; do cp /run/fb/Xvfb_screen0 /run/cap/f$(printf %04d $i).xwd; i=$((i+1)); sleep 0.1; done
      date +%s.%N > /run/cap/end ) &
    C=$!
    sleep 1.5; xdotool mousemove 62 11 mousedown 1 sleep 0.25 mouseup 1
    sleep 2;   xdotool mousemove 140 97 mousedown 1 sleep 0.2 mousemove 160 97 sleep 0.2 mousemove 178 97 sleep 0.4 mouseup 1
    sleep 1.5; xdotool mousemove 110 157 mousedown 1 sleep 0.2 mousemove 150 157 sleep 0.2 mousemove 178 157 sleep 0.4 mouseup 1
    sleep 1.5; xdotool mousemove 338 33 mousedown 1 sleep 0.2 mouseup 1
    sleep 1;   xdotool mousemove 122 11 mousedown 1 sleep 0.2 mouseup 1
    sleep 2;   xdotool mousemove 840 255; sleep 3; xdotool mousemove 330 300; sleep 3; xdotool mousemove 690 120
    wait $C'
n=$(ls "$run/cap"/f*.xwd | wc -l)
secs=$(awk -v a="$(cat "$run/cap/start")" -v z="$(cat "$run/cap/end")" 'BEGIN{printf "%.3f", z-a}')
fps=$(awk -v n="$n" -v s="$secs" 'BEGIN{printf "%.3f", n/s}')
echo "擷取 $n 張、$secs 秒，約 $fps fps"
timeout 300 docker run "${lim[@]}" --name "dgrs-promo-setenc-$lang" --memory 2g --cpus 2 --pids-limit 64 \
  -v "$run/cap:/cap:ro" -v "$WP/promo/$lang:/out" kgb-cht-video:1 \
  ffmpeg -y -loglevel error -nostdin -framerate "$fps" -i /cap/f%04d.xwd -vf "crop=960:660:0:0,format=yuv420p" \
    -r 30 -c:v libx264 -preset veryfast -crf 18 /out/settings.mp4
rm -f "$run"/cap/f*.xwd
cat "$run/scratch/../frontend.cfg" 2>/dev/null || true
ls -la "$WP/promo/$lang/settings.mp4"
