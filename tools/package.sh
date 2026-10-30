#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-RRSAL-1.0
# 發行打包（~/cht/AGENTS_DOSGOLEM_CHT.md §15.1，Rust 版）。建置全部在 Docker，主機只做檢查與組檔。
#
#   tools/package.sh image                       建打包映像（唯一需要網路的一步，另抓 AppImage runtime）
#   tools/package.sh with-data [linux|windows|macos|all]
#                                                本機完整版：內含原版光碟資料，解開就能玩。
#                                                只放 workplace/dist/，不進 Git、不上傳。
#
# 版本取自 git describe；本庫與 FerroGolem 的樹都要乾淨且 HEAD 已推送。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WP="$ROOT/workplace"
FG_ROOT="${FG_ROOT:-$(cd "$ROOT/../ferrogolem" 2>/dev/null && pwd)}"
PKG_IMAGE="kgb-cht-pkg:1.99.1-r1"
FONT_SRC=/usr/share/fonts/opentype/noto/NotoSansCJK-Medium.ttc
FONT_LICENSE=/usr/share/doc/fonts-noto-cjk/copyright
# AppImage type2 runtime：第一次 image 時下載到 workplace/cache，之後只核對雜湊。
RUNTIME_URL="https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-x86_64"
RUNTIME="$WP/cache/appimage-runtime-x86_64"
MACOS_MIN=11.0
NAME=kgb-cht

die() { echo "package.sh：$*" >&2; exit 1; }
need() { test -e "$1" || die "缺 $1"; }

run_pkg() {  # 在打包映像內執行（離線、非 root、資源上限）。
  timeout 3600 docker run --rm -u "$(id -u):$(id -g)" --name "dgrs-pkg-$$" \
    --network none --memory 8g --cpus 6 --pids-limit 1024 \
    --log-opt max-size=10m --log-opt max-file=3 \
    -e CARGO_HOME=/cache -e CARGO_TARGET_DIR=/target -e HOME=/tmp -e MACOSX_DEPLOYMENT_TARGET=$MACOS_MIN \
    -v "$WP/cargo:/cache" -v "$WP/target-pkg:/target" -v "$FG_ROOT:/ferrogolem:ro" -v "$ROOT:/project:ro" \
    -w /project "$PKG_IMAGE" bash -c "$1"
}

preflight() {
  test "$(git -C "$ROOT" config user.email)" = wicanr2@gmail.com || die "git 身分不是 wicanr2@gmail.com"
  for r in "$ROOT" "$FG_ROOT"; do
    test -z "$(git -C "$r" status --porcelain)" || die "$r 有未提交的變更"
    git -C "$r" fetch -q origin 2>/dev/null || true
    git -C "$r" branch -r --contains HEAD | grep -q . || die "$r 的 HEAD 尚未推送"
  done
  docker image inspect "$PKG_IMAGE" >/dev/null 2>&1 || die "缺映像 $PKG_IMAGE，先跑 tools/package.sh image"
  need "$FONT_SRC"; need "$FONT_LICENSE"
  need "$WP/disc/cd/CSPPRG.EXE"; need "$WP/disc/CSP.CFG"; need "$WP/disc/manifest.json"
  test "$(sha256sum "$WP/disc/cd/CSPPRG.EXE" | cut -c1-64)" = d2fb547ec2516328915a320fcc2ea2330457533a8ae222e1c9821d56f5aac47c \
    || die "原版 CSPPRG.EXE 雜湊不符"
  VERSION="$(git -C "$ROOT" describe --tags --always)"
  FG_COMMIT="$(git -C "$FG_ROOT" rev-parse --short HEAD)"
  echo "版本 $VERSION，FerroGolem $FG_COMMIT"
}

check_text() {
  # 譯文表載入與簡中產物一致（同 tools/rust.sh 的 kgb-zhcn --check）。
  run_pkg 'cargo build --release --locked --offline -p kgb-cht --bin kgb-zhcn -q && /target/release/kgb-zhcn --check /project/text'
}

build() {  # $1＝rust target
  local t=$1 env=""
  case "$t" in
    x86_64-pc-windows-gnu) env="CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc" ;;
    x86_64-apple-darwin) env='CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=x86_64-apple-$OSXCROSS_TARGET-clang CC_x86_64_apple_darwin=x86_64-apple-$OSXCROSS_TARGET-clang' ;;
    aarch64-apple-darwin) env='CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=arm64-apple-$OSXCROSS_TARGET-clang CC_aarch64_apple_darwin=arm64-apple-$OSXCROSS_TARGET-clang' ;;
  esac
  run_pkg "export OSXCROSS_TARGET=\$(osxcross-conf 2>/dev/null | sed -n 's/^export OSXCROSS_TARGET=//p'); env $env \
    cargo build --release --locked --offline -p kgb-cht --bin kgb-play --target $t"
}

stage_common() {  # $1＝資料根目錄（放 text/、font/、original/ 與文件）
  local d=$1
  mkdir -p "$d/text" "$d/font" "$d/original/cd"
  cp "$ROOT"/text/*.zh-TW.tsv "$ROOT"/text/*.zh-CN.tsv "$ROOT/text/sprite-labels.tsv" "$ROOT/text/card-labels.tsv" "$d/text/"
  cp "$FONT_SRC" "$d/font/"
  cp "$FONT_LICENSE" "$d/font/NotoSansCJK-LICENSE.txt"
  cp -r "$WP/disc/cd/." "$d/original/cd/"
  cp "$WP/disc/CSP.CFG" "$d/original/"
  test "$(ls "$d/original/cd" | wc -l)" -ge 180 || die "原版光碟檔數量不足"
}

stage_docs() {  # $1＝放文件的目錄
  cp "$ROOT/LICENSE" "$1/LICENSE.txt"
  cat > "$1/讀我.txt" <<EOF
KGB（Cryo，1992，CD 版 CONSPIRACY）繁體中文化 $VERSION（本機完整版，內含原版資料，請勿散布）

執行：直接開啟 $NAME（Windows 為 $NAME.exe；macOS 為 KGB-CHT.app，第一次請按右鍵選「打開」）。
工具列可切換 繁中、简中、英文原版。存檔放在使用者資料目錄的 kgb-cht/save。
模擬器：FerroGolem $FG_COMMIT。授權：自有程式 RRSAL-1.0（LICENSE.txt）；字型 Noto Sans CJK，SIL OFL 1.1（font/）。
原版遊戲著作權屬 Cryo Interactive、Virgin Games 及其權利繼承者；本專案與其無隸屬或授權關係。
EOF
}

linux() {
  build x86_64-unknown-linux-gnu
  local app="$OUT/linux/AppDir" share
  share="$app/usr/share/$NAME"
  mkdir -p "$app/usr/bin" "$share"
  cp "$WP/target-pkg/x86_64-unknown-linux-gnu/release/kgb-play" "$app/usr/bin/$NAME"
  stage_common "$share"; stage_docs "$share"
  printf '#!/bin/sh\nexec "$(dirname "$(readlink -f "$0")")/usr/bin/%s" "$@"\n' "$NAME" > "$app/AppRun"
  chmod +x "$app/AppRun"
  printf '[Desktop Entry]\nType=Application\nName=KGB 中文版\nExec=%s\nIcon=%s\nCategories=Game;\n' "$NAME" "$NAME" > "$app/$NAME.desktop"
  need "$RUNTIME"
  # mksquashfs 要寫輸出目錄，另以可寫掛載執行（run_pkg 的專案掛載是唯讀）。
  timeout 1800 docker run --rm -u "$(id -u):$(id -g)" --name "dgrs-pkg-sq-$$" --network none --memory 4g --cpus 4 --pids-limit 256 \
    --log-opt max-size=10m --log-opt max-file=3 -v "$OUT/linux:/out" "$PKG_IMAGE" \
    mksquashfs /out/AppDir /out/fs.squashfs -root-owned -noappend -comp zstd -quiet
  cat "$RUNTIME" "$OUT/linux/fs.squashfs" > "$OUT/$NAME-$VERSION-linux-x86_64-with-data.AppImage"
  chmod +x "$OUT/$NAME-$VERSION-linux-x86_64-with-data.AppImage"
  rm -rf "$OUT/linux"
}

windows() {
  build x86_64-pc-windows-gnu
  local d="$OUT/windows/$NAME"
  mkdir -p "$d"
  cp "$WP/target-pkg/x86_64-pc-windows-gnu/release/kgb-play.exe" "$d/$NAME.exe"
  stage_common "$d"; stage_docs "$d"
  (cd "$OUT/windows" && zip -qr "$OUT/$NAME-$VERSION-windows-x86_64-with-data.zip" "$NAME")
  rm -rf "$OUT/windows"
}

macos() {
  build x86_64-apple-darwin
  build aarch64-apple-darwin
  local app="$OUT/macos/KGB-CHT.app/Contents"
  mkdir -p "$app/MacOS" "$app/Resources"
  timeout 600 docker run --rm -u "$(id -u):$(id -g)" --name "dgrs-pkg-lipo-$$" --network none --memory 1g --cpus 1 --pids-limit 64 \
    --log-opt max-size=10m --log-opt max-file=3 -v "$WP/target-pkg:/target:ro" -v "$app/MacOS:/out" "$PKG_IMAGE" bash -c \
    "T=x86_64-apple-\$(osxcross-conf | sed -n 's/^export OSXCROSS_TARGET=//p'); \$T-lipo -create \
      /target/x86_64-apple-darwin/release/kgb-play /target/aarch64-apple-darwin/release/kgb-play -output /out/$NAME"
  stage_common "$app/Resources"; stage_docs "$app/Resources"
  cat > "$app/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>$NAME</string>
<key>CFBundleIdentifier</key><string>io.github.wicanr2.kgb-cht</string>
<key>CFBundleName</key><string>KGB 中文版</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>${VERSION#v}</string>
<key>LSMinimumSystemVersion</key><string>$MACOS_MIN</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
EOF
  cp "$app/Resources/讀我.txt" "$OUT/macos/"
  (cd "$OUT/macos" && zip -qry "$OUT/$NAME-$VERSION-macos-universal-with-data.zip" KGB-CHT.app 讀我.txt)
  rm -rf "$OUT/macos"
}

verify() {  # 架構與內容檢查
  local f
  timeout 600 docker run --rm -u "$(id -u):$(id -g)" --name "dgrs-pkg-v-$$" --network none --memory 2g --cpus 2 --pids-limit 128 \
    --log-opt max-size=10m --log-opt max-file=3 -v "$OUT:/out:ro" -v "$WP/target-pkg:/target:ro" "$PKG_IMAGE" bash -c '
    set -e
    T=x86_64-apple-$(osxcross-conf 2>/dev/null | sed -n "s/^export OSXCROSS_TARGET=//p")
    test -f /target/x86_64-unknown-linux-gnu/release/kgb-play && file /target/x86_64-unknown-linux-gnu/release/kgb-play | grep -q "ELF 64-bit"
    test -f /target/x86_64-pc-windows-gnu/release/kgb-play.exe && file /target/x86_64-pc-windows-gnu/release/kgb-play.exe | grep -q "PE32+"
    for a in x86_64 aarch64; do test -f /target/$a-apple-darwin/release/kgb-play || continue
      file /target/$a-apple-darwin/release/kgb-play | grep -q Mach-O
      $T-otool -L /target/$a-apple-darwin/release/kgb-play | tail -n +2 | awk "{print \$1}" | grep -vE "^(/usr/lib/|/System/Library/)" && exit 1 || true
    done
    test -f /target/aarch64-apple-darwin/release/kgb-play && { $T-otool -l /target/aarch64-apple-darwin/release/kgb-play | grep -q LC_CODE_SIGNATURE; }
    echo 架構檢查通過'
  for f in "$OUT"/*; do
    case "$f" in
      *.zip) unzip -l "$f" | grep -q "original/cd/CSPPRG.EXE" || die "$f 缺原版" ; unzip -l "$f" | grep -q "text/chapter4.zh-TW.tsv" || die "$f 缺譯文" ;;
      *.AppImage) test "$(head -c 4 "$f" | od -An -c | tr -d ' ')" = '177ELF' || die "$f 不是 ELF" ;;
    esac
    echo "$(du -h "$f" | cut -f1)  $(basename "$f")"
  done
}

case "${1:-}" in
  image)
    docker build -f "$ROOT/tools/Dockerfile.package" -t "$PKG_IMAGE" "$ROOT/tools"
    mkdir -p "$WP/cache"
    test -f "$RUNTIME" || timeout 300 docker run --rm -u "$(id -u):$(id -g)" --name "dgrs-pkg-dl-$$" --network bridge \
      --memory 512m --cpus 1 --pids-limit 64 --log-opt max-size=10m --log-opt max-file=3 -v "$WP/cache:/out" "$PKG_IMAGE" \
      curl -fsSL -o /out/appimage-runtime-x86_64 "$RUNTIME_URL"
    echo "AppImage runtime SHA-256：$(sha256sum "$RUNTIME" | cut -c1-64)"
    ;;
  with-data)
    preflight
    check_text
    OUT="$WP/dist/$VERSION"
    rm -rf "$OUT"; mkdir -p "$OUT" "$WP/target-pkg"
    case "${2:-all}" in
      linux) linux ;; windows) windows ;; macos) macos ;;
      all) linux; windows; macos ;;
      *) die "平台只接受 linux、windows、macos、all" ;;
    esac
    verify
    ;;
  *) sed -n 3,10p "$0"; exit 2 ;;
esac
