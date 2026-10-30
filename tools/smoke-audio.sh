#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
# 發行包聲音驗收：解開 Linux AppImage，在 kgb-cht-audiocheck 映像內以 Xvfb 執行，聲音送到
# PulseAudio 虛擬喇叭（有時鐘），錄下 monitor 成 rec.wav，結束時截圖 shot.xwd。
# ALSA 的 null 裝置沒有時鐘，播放端會全速空轉寫出巨量資料，不要用它代替。
#
#   tools/smoke-audio.sh <AppImage 檔名（dist-all 內）> <輸出名> [秒數，預設 420]
# 建映像：DOCKER_BUILDKIT=0 docker build --network host -t kgb-cht-audiocheck:1 -f tools/Dockerfile.audiocheck tools
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app="${1:?AppImage 檔名}"; name="${2:?輸出名}"; secs="${3:-420}"
test -f "$ROOT/dist-all/$app" || { echo "找不到 dist-all/$app" >&2; exit 1; }
docker image inspect kgb-cht-audiocheck:1 >/dev/null 2>&1 || { echo "先建 kgb-cht-audiocheck:1（見檔頭）" >&2; exit 1; }
run="$ROOT/workplace/runs/$name"
test ! -e "$run" || { echo "$run 已存在，換一個輸出名" >&2; exit 1; }
mkdir -p "$run/fb" "$run/home/xdg"
chmod 700 "$run/home/xdg"
printf 'pcm.!default { type pulse }\nctl.!default { type pulse }\n' > "$run/home/.asoundrc"
lim=(--rm -u "$(id -u):$(id -g)" --network none --log-opt max-size=10m --log-opt max-file=3)
timeout 600 docker run "${lim[@]}" --name "dgrs-sa-sq-$name" --memory 2g --cpus 2 --pids-limit 128 \
  -v "$ROOT/dist-all:/dist:ro" -v "$run:/out" kgb-cht-pkg:1.99.1-r2 \
  unsquashfs -q -o 944632 -d /out/root "/dist/$app" >/dev/null
timeout $((secs + 120)) docker run "${lim[@]}" --init --name "dgrs-sa-$name" --memory 3g --cpus 3 --pids-limit 256 \
  -e HOME=/run/home -e XDG_RUNTIME_DIR=/run/home/xdg -e WGPU_BACKEND=vulkan -e LANG=zh_TW.UTF-8 \
  -v "$run:/run" kgb-cht-audiocheck:1 sh -c "
    pulseaudio --daemonize=no --exit-idle-time=-1 --disallow-exit -n \
      --load='module-null-sink sink_name=out rate=44100' --load=module-native-protocol-unix >/run/pulse.log 2>&1 &
    sleep 3; pactl set-default-sink out
    parecord --device=out.monitor --file-format=wav --rate=44100 --channels=2 /run/rec.wav & R=\$!
    Xvfb :99 -screen 0 1000x700x24 -fbdir /run/fb >/run/xvfb.log 2>&1 & sleep 2
    DISPLAY=:99 /run/root/AppRun >/run/play.log 2>&1 & P=\$!
    sleep $secs; cp /run/fb/Xvfb_screen0 /run/shot.xwd; kill \$P; sleep 1; kill \$R; sleep 1"
ls -la "$run/rec.wav"
