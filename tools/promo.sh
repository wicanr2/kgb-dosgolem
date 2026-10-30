#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
# 推廣影片（~/cht/AGENTS_DOSGOLEM_CHT.md §15.3）：中文化實跑截圖＋實機片段＋標題卡＋字幕，1280×720。
# 在 kgb-cht-video 映像內合成（--network none、唯讀掛載素材），輸出 dist-all/kgb-cht-<版本>-promo-<語言>.mp4。
# 配樂：workplace/promo/music/bgm.wav（kgb-run --wav 從模擬器錄下的原版 AdLib 與數位音訊）存在時循環鋪滿全片，淡入淡出；片尾卡註明權利歸屬。
#
#   tools/promo.sh [zh-TW|zh-CN|all]          合成（預設 all）。繁中截圖取自 docs/images/，簡中取自 workplace/promo/zh-CN/；
#                                             兩者另需 workplace/promo/<語言>/intro-card.png 與 gameplay.mp4
#   tools/promo.sh gameplay <目錄> <語言>     把 kgb-run --frames 的 frames.rgb 編成 workplace/promo/<語言>/gameplay.mp4（§15.3a）
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTDIR="$ROOT/workplace/promo"
# 檔名用最近的版本標籤，與 dist-all 的完整版對齊。
VERSION="$(git -C "$ROOT" describe --tags --abbrev=0)"
mkdir -p "$OUTDIR/zh-TW" "$OUTDIR/zh-CN" "$ROOT/dist-all"

if [ "${1:-}" = gameplay ]; then
  src="$(cd "${2:?畫格目錄}" && pwd)"; lang="${3:?語言 zh-TW 或 zh-CN}"
  test -f "$src/frames.rgb" && test -f "$src/frames.txt" || { echo "$src 缺 frames.rgb 或 frames.txt" >&2; exit 1; }
  read -r fw fh n < "$src/frames.txt"
  echo "畫格 ${fw}×${fh}，${n} 格"
  # 錄製間隔 66,667 步、cycles 1000（每秒 100 萬步）＝每秒 15 格遊戲時間。
  timeout 1200 docker run --rm -u "$(id -u):$(id -g)" --name "dgrs-promo-gp-$$" --network none \
    --memory 2g --cpus 2 --pids-limit 256 --log-opt max-size=10m --log-opt max-file=3 \
    -v "$src:/src:ro" -v "$OUTDIR/$lang:/out" kgb-cht-video:1 \
    ffmpeg -y -loglevel error -nostdin -f rawvideo -pix_fmt rgb24 -s "${fw}x${fh}" -r 15 -i /src/frames.rgb \
      -threads 2 -c:v libx264 -preset veryfast -crf 18 -pix_fmt yuv420p /out/gameplay.mp4
  ls -la "$OUTDIR/$lang/gameplay.mp4"
  exit 0
fi

render() {  # $1＝語言
  local lang=$1 img
  case "$lang" in zh-TW) img="$ROOT/docs/images" ;; zh-CN) img="$OUTDIR/zh-CN" ;; *) echo "語言只接受 zh-TW、zh-CN" >&2; exit 1 ;; esac
  for f in "$OUTDIR/$lang/intro-card.png" "$OUTDIR/$lang/gameplay.mp4" "$img/ch1-office-narration.png"; do
    test -f "$f" || { echo "缺 $f" >&2; exit 1; }
  done
  mkdir -p "$OUTDIR/music"
  rm -f "$ROOT/dist-all"/kgb-cht-*-promo-"$lang".mp4 "$ROOT/dist-all"/kgb-cht-*-promo.mp4
  timeout 1200 docker run --rm -i -u "$(id -u):$(id -g)" --name "dgrs-promo-$lang-$$" --network none \
    --memory 3g --cpus 2 --pids-limit 256 --log-opt max-size=10m --log-opt max-file=3 \
    -v "$img:/img:ro" -v "$OUTDIR/music:/music:ro" -v "$OUTDIR/$lang:/extra:ro" -v "$OUTDIR:/out" -v "$ROOT/dist-all:/dist" \
    -e VERSION="$VERSION" -e PL="$lang" kgb-cht-video:1 bash -s <<'SCRIPT'
set -eu
# 設計 token：取自遊戲。金＝名單字色 (210,172,100)；磚紅與暗棕＝片頭名單背景；紅＝紅星。
BG1='#3a1a10'; BG2='#0d0604'; GOLD='#d2ac64'; GOLDSH='#5a3a10'; RED='#b81c1c'; CREAM='#f2e6cc'
if [ "$PL" = zh-CN ]; then
  FB=Noto-Serif-CJK-SC-Bold; FR=Noto-Sans-CJK-SC   # ImageMagick 字型名（convert -list font），SC 字形
  NAME='KGB 阴谋'; T00='简体中文版'; S00='Cryo《KGB》CD 版（Conspiracy）'
  Q1='一个冷门经典的汉化请求'; Q1B='来自 bilibili 伙伴 索菲娅（ソフィア）'
  C02='开场字卡与字幕都换成中文'; C03='第一章　莫斯科，KGB P 处'; C04='对话菜单与回答都是中文'
  C05='游标经过场景，动作提示也是中文'; GP='实机游玩：翻旁白、游标扫过办公室、和别洛夫交谈（2 倍速）'
  C06='第二章　列宁格勒，戈斯季尼察饭店'; C07='第四章　码头，八月政变前夕'; C08='倒带重播时的框内信息'
  C09='片头名单跟着原版淡入淡出'; C10='片尾演员表'
  T11='繁中・简中・英文原版 随时切换'; S11='github.com/wicanr2/kgb-dosgolem　RRSAL-1.0　不附原版游戏'
  Q12='原版游戏著作权属 Cryo Interactive、Virgin Games 及其权利继承者'; Q12B="配乐取自原版游戏录音，著作权属原权利人　${VERSION}"
  SHOT05=/img/ch1-cursor.png
else
  FB=Noto-Serif-CJK-TC-Bold; FR=Noto-Sans-CJK-TC   # TC 字形
  NAME='KGB 陰謀'; T00='繁體中文版'; S00='Cryo《KGB》CD 版（Conspiracy）'
  Q1='一個冷門經典的中文化請求'; Q1B='來自 bilibili 夥伴 索菲婭（ソフィア）'
  C02='開場字卡與字幕都換成中文'; C03='第一章　莫斯科，KGB P 處'; C04='對話選單與回答都是中文'
  C05='動作選單與游標動詞，連圖片上的字也換成中文'; GP='實機遊玩：翻旁白、游標掃過辦公室、和別洛夫交談（2 倍速）'
  C06='第二章　列寧格勒，戈斯季尼察飯店'; C07='第四章　碼頭，八月政變前夕'; C08='倒帶重播時的框內訊息'
  C09='片頭名單跟著原版淡入淡出'; C10='片尾演員表'
  T11='繁中・简中・英文原版 隨時切換'; S11='github.com/wicanr2/kgb-dosgolem　RRSAL-1.0　不附原版遊戲'
  Q12='原版遊戲著作權屬 Cryo Interactive、Virgin Games 及其權利繼承者'; Q12B="配樂取自原版遊戲錄音，著作權屬原權利人　${VERSION}"
  SHOT05=/img/ch1-action-menu.png
fi
export HOME=/tmp   # fontconfig 快取
for f in "$FB" "$FR"; do convert -list font 2>/dev/null | grep -qx "  Font: $f" || { echo "缺字型 $f" >&2; exit 1; }; done
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
gameplay() {  # 實機片段：2 倍速，放在與截圖頁相同的金框位置，下方字幕條
  convert -size ${W}x${H} "gradient:${BG1}-${BG2}" -fill "$GOLD" -draw "rectangle 157,11 1122,616" \
    -fill '#000000b0' -draw "rectangle 0,628 ${W},720" \
    -font "$FR" -fill "$CREAM" -gravity south -pointsize 36 -annotate +0+26 "$1" $T/gp-bg.png
  local d fo
  d=$(ffprobe -v error -show_entries format=duration -of csv=p=0 /extra/gameplay.mp4 | awk '{printf "%.2f", $1/2}')
  fo=$(awk "BEGIN{print $d-0.6}")
  ffmpeg -y -loglevel error -nostdin -loop 1 -i $T/gp-bg.png -i /extra/gameplay.mp4 -filter_complex \
    "[1:v]setpts=0.5*PTS,fps=$FPS[g];[0:v][g]overlay=160:14:shortest=1,fade=t=in:st=0:d=0.6,fade=t=out:st=$fo:d=0.6,format=yuv420p" \
    -t "$d" -r $FPS -threads 2 -filter_threads 1 -c:v libx264 -preset veryfast -pix_fmt yuv420p $T/05g.mp4
}

card  $T/00.png "$NAME" "$T00" "$S00"
quote $T/01.png "$Q1" "$Q1B"
slide $T/02.png /extra/intro-card.png "$C02"
slide $T/03.png /img/ch1-office-narration.png "$C03"
slide $T/04.png /img/ch1-dialog-menu.png "$C04"
slide $T/05.png "$SHOT05" "$C05"
slide $T/06.png /img/ch2-room205.png "$C06"
slide $T/07.png /img/ch4-docks.png "$C07"
slide $T/08.png /img/ch1-replay-box.png "$C08"
slide $T/09.png /img/credits-opening.png "$C09"
slide $T/10.png /img/credits-cast.png "$C10"
card  $T/11.png "$NAME" "$T11" "$S11"
quote $T/12.png "$Q12" "$Q12B"

: > $T/list.txt
for f in 00:6 01:5 02:6 03:6 04:5.5 05:5.5 06:5.5 07:5.5 08:5 09:5 10:5 11:6 12:4; do
  n=${f%%:*}; s=${f##*:}; clip $T/$n.png "$s"; echo "file '$T/$n.mp4'" >> $T/list.txt
  if [ "$n" = 05 ]; then gameplay "$GP"; echo "file '$T/05g.mp4'" >> $T/list.txt; fi
done
OUT="/dist/kgb-cht-${VERSION}-promo-${PL}.mp4"
ffmpeg -y -loglevel error -nostdin -f concat -safe 0 -i $T/list.txt -c copy $T/silent.mp4
if [ -f /music/bgm.wav ]; then
  # 配樂先無限循環再剪到影片長度（避免 -shortest 截掉片尾卡），淡入 2 秒、淡出 3 秒。
  DUR=$(ffprobe -v error -show_entries format=duration -of csv=p=0 $T/silent.mp4); FO=$(awk "BEGIN{print $DUR-3}")
  ffmpeg -y -loglevel error -nostdin -i $T/silent.mp4 -i /music/bgm.wav -filter_complex \
    "[1:a]aloop=loop=-1:size=2000000000,atrim=0:$DUR,afade=t=in:st=0:d=2,afade=t=out:st=$FO:d=3[a]" \
    -map 0:v -map "[a]" -c:v copy -c:a aac -b:a 160k -movflags +faststart "$OUT"
else
  cp $T/silent.mp4 "$OUT"
fi
for t in 3 9 38 48 85; do ffmpeg -y -loglevel error -nostdin -ss $t -i "$OUT" -frames:v 1 "/out/check-${PL}-$t.png"; done
echo "$(basename "$OUT") $(ffprobe -v error -show_entries format=duration -of csv=p=0 "$OUT") 秒"
SCRIPT
}

case "${1:-all}" in
  zh-TW|zh-CN) render "$1" ;;
  all) render zh-TW; render zh-CN ;;
  *) sed -n 3,9p "$0"; exit 2 ;;
esac
