#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
# 推廣影片（~/cht/AGENTS_DOSGOLEM_CHT.md §15.3）：中文化實跑截圖＋標題卡＋字幕，靜態加淡入淡出，1280×720。
# 在 kgb-cht-video 映像內合成（--network none、唯讀掛載截圖），輸出 dist-all/kgb-cht-<版本>-promo.mp4。
# 音訊輸出尚未完成（issue #3），本版無配樂。
#
#   tools/promo.sh            用 docs/images/ 與 workplace/promo/shots/ 的截圖合成
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTDIR="$ROOT/workplace/promo"
VERSION="$(git -C "$ROOT" describe --tags --always)"
mkdir -p "$OUTDIR/shots" "$ROOT/dist-all"
# 成品與完整版一起集中放 dist-all/（.gitignore），只留目前版本；檢查畫格留在 workplace/promo。
rm -f "$ROOT/dist-all"/kgb-cht-*-promo.mp4
test -f "$OUTDIR/shots/intro-card.png" || { echo "缺 $OUTDIR/shots/intro-card.png（開場字卡截圖）" >&2; exit 1; }

timeout 1200 docker run --rm -i -u "$(id -u):$(id -g)" --name "dgrs-promo-$$" --network none \
  --memory 3g --cpus 2 --pids-limit 256 --log-opt max-size=10m --log-opt max-file=3 \
  -v "$ROOT/docs/images:/img:ro" -v "$OUTDIR/shots:/extra:ro" -v "$OUTDIR:/out" -v "$ROOT/dist-all:/dist" \
  -e VERSION="$VERSION" kgb-cht-video:1 bash -s <<'SCRIPT'
set -eu
# 設計 token：取自遊戲。金＝名單字色 (210,172,100)；磚紅與暗棕＝片頭名單背景；紅＝紅星。
BG1='#3a1a10'; BG2='#0d0604'; GOLD='#d2ac64'; GOLDSH='#5a3a10'; RED='#b81c1c'; CREAM='#f2e6cc'
FB=/usr/share/fonts/opentype/noto/NotoSerifCJK-Bold.ttc
FR=/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc
for f in "$FB" "$FR"; do test -f "$f" || { echo "缺字型 $f" >&2; exit 1; }; done
W=1280; H=720; FPS=25; T=/tmp/p; mkdir -p "$T"

card() {  # $1 輸出 $2 大標 $3 中標 $4 小標
  convert -size ${W}x${H} "radial-gradient:${BG1}-${BG2}" \
    -fill "$RED" -draw "polygon 640,70 657,118 708,118 667,148 683,196 640,166 597,196 613,148 572,118 623,118" \
    -font "$FB" -gravity center \
    -fill "$GOLDSH" -pointsize 120 -annotate +4-6+4 "$2" -fill "$GOLD" -pointsize 120 -annotate +0-10 "$2" \
    -fill "$CREAM" -pointsize 46 -annotate +0+110 "$3" \
    -font "$FR" -fill "$GOLD" -pointsize 28 -annotate +0+190 "$4" "$1"
}
slide() {  # $1 輸出 $2 截圖 $3 字幕
  convert -size ${W}x${H} "gradient:${BG1}-${BG2}" "$T/bg.png"
  convert "$2" -resize x600 -bordercolor "$GOLD" -border 3 "$T/sc.png"
  convert "$T/bg.png" "$T/sc.png" -gravity north -geometry +0+14 -composite \
    -fill '#000000b0' -draw "rectangle 0,628 ${W},720" \
    -font "$FR" -fill "$CREAM" -gravity south -pointsize 36 -annotate +0+26 "$3" "$1"
}
quote() {  # $1 輸出 $2 主文 $3 附註
  convert -size ${W}x${H} "gradient:${BG1}-${BG2}" -font "$FB" \
    -fill "$CREAM" -pointsize 50 -gravity center -annotate +0-20 "$2" \
    -font "$FR" -fill "$GOLD" -pointsize 30 -annotate +0+90 "$3" "$1"
}
clip() {  # $1 png $2 秒
  local fo; fo=$(awk "BEGIN{print $2-0.6}")
  ffmpeg -y -loglevel error -nostdin -loop 1 -i "$1" -t "$2" -r $FPS \
    -vf "fade=t=in:st=0:d=0.6,fade=t=out:st=$fo:d=0.6,format=yuv420p" \
    -threads 2 -filter_threads 1 -c:v libx264 -preset veryfast -pix_fmt yuv420p "${1%.png}.mp4"
}

card  $T/00.png 'KGB 陰謀' '繁體中文版' 'Cryo《KGB》CD 版（Conspiracy）'
quote $T/01.png '一個冷門經典的中文化請求' '來自 bilibili 夥伴 索菲婭（ソフィア）'
slide $T/02.png /extra/intro-card.png '開場字卡與字幕都換成中文'
slide $T/03.png /img/ch1-office-narration.png '第一章　莫斯科，KGB P 處'
slide $T/04.png /img/ch1-dialog-menu.png '對話選單與回答都是中文'
slide $T/05.png /img/ch1-action-menu.png '動作選單與游標動詞，連圖片上的字也換成中文'
slide $T/06.png /img/ch2-room205.png '第二章　列寧格勒，戈斯季尼察飯店'
slide $T/07.png /img/ch4-docks.png '第四章　碼頭，八月政變前夕'
slide $T/08.png /img/ch1-replay-box.png '倒帶重播時的框內訊息'
slide $T/09.png /img/credits-opening.png '片頭名單跟著原版淡入淡出'
slide $T/10.png /img/credits-cast.png '片尾演員表'
card  $T/11.png 'KGB 陰謀' '繁中・简中・英文原版 隨時切換' 'github.com/wicanr2/kgb-dosgolem　RRSAL-1.0　不附原版遊戲'
quote $T/12.png '原版遊戲著作權屬 Cryo Interactive、Virgin Games 及其權利繼承者' "本片不含原版音樂　${VERSION}"

: > $T/list.txt
for f in 00:6 01:5 02:6 03:6 04:5.5 05:5.5 06:5.5 07:5.5 08:5 09:5 10:5 11:6 12:4; do
  n=${f%%:*}; s=${f##*:}; clip $T/$n.png "$s"; echo "file '$T/$n.mp4'" >> $T/list.txt
done
ffmpeg -y -loglevel error -nostdin -f concat -safe 0 -i $T/list.txt -c copy -movflags +faststart "/dist/kgb-cht-${VERSION}-promo.mp4"
for t in 3 9 27 66; do ffmpeg -y -loglevel error -nostdin -ss $t -i "/dist/kgb-cht-${VERSION}-promo.mp4" -frames:v 1 "/out/check-$t.png"; done
ffprobe -v error -show_entries format=duration -of csv=p=0 "/dist/kgb-cht-${VERSION}-promo.mp4"
SCRIPT
