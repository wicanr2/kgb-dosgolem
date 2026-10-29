# 目前狀態

核對日期：2026-10-29。

## 現況

- 原版光碟已解到 `workplace/disc/`（docs/re/001）。
- 模擬器在 FerroGolem（`~/cht/ferrogolem`）。KGB 在 50M 步內跑到 `INTRO.BGF` 開場，沒有停點（docs/re/002）。
- 文字 routine 已接手（docs/spec/004）：開場兩段共 47 句字幕已有譯文（`text/intro.zh-TW.tsv`，譯名見 `text/glossary-zh-TW.tsv`），第一段 31 句在 334M 步內全部以中文顯示，換場時依原版時機消失。
- FerroGolem 補上 SB D0h 與滑鼠後，KGB 1.2B 步進入第一章辦公室場景；章節標題、地點標籤、旁白已接手（無譯文時以英文畫在覆繪層），時鐘由原版照畫。
- 第一章文字已能萃取：以遊戲解碼器取出 CHAP1U 全部 3,999 個片語（docs/re/004），尚待翻譯。SQX 解壓工具已驗證（docs/re/003）。
- 可開視窗遊玩：`tools/rust.sh play`（FerroGolem GUI 前端），Xvfb 無頭驗證中文字幕正確（`gui-k1`、`gui-k2`）。時脈改為 cycles 20000 後開場以正常速度播放。
- 對拍基準 dosgolem-rust 分支 `kgb-oracle` 已跑出完整開場，6 句字幕與攔到的事件逐字相同（FerroGolem docs/re/001）。開場字卡「Moscow, August 1991...」不經文字 routine，推測是語系圖檔（worklist `image-localization`）。
- 未完成項見 `docs/worklist.json`，以 `tools/worklist.py verify` 檢查。

## 術語

| 術語 | 意思 |
|---|---|
| FerroGolem | 自有 clean-room RRSAL Rust DOS 模擬器，另一個 repo |
| 對拍基準 | `~/cht/dosgolem-rust` 分支 `kgb-oracle`（rust-dos） |
| 文字事件 | 被接手的一次繪字呼叫：原文、呼叫點、座標、字型狀態 |
| 比對鍵 | 原版英文原文，區塊去尾端 0、中間 0 寫成 `^` |
| 覆繪層 | 放大 3 倍（960×600）合成的中文文字層 |
| 字幕 | 開場在畫面底部（y＝181 起）的英文句子 |
