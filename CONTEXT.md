# 目前狀態

核對日期：2026-10-29。

## 現況

- 原版光碟已解到 `workplace/disc/`（docs/re/001）。
- 模擬器在 FerroGolem（`~/cht/ferrogolem`）。KGB 在 50M 步內跑到 `INTRO.BGF` 開場，沒有停點（docs/re/002）。
- 文字 routine 已接手（docs/spec/004）：開場兩段共 47 句字幕已有譯文（`text/intro.zh-TW.tsv`，譯名見 `text/glossary.tsv`），第一段 31 句在 334M 步內全部以中文顯示，換場時依原版時機消失。
- FerroGolem 補上 SB D0h 與滑鼠後，KGB 1.2B 步進入第一章辦公室場景；章節標題、地點標籤、旁白已接手（無譯文時以英文畫在覆繪層），時鐘由原版照畫。
- 第一章片語已全部翻譯：以遊戲解碼器取出 CHAP1U 全部 3,999 個片語（docs/re/004），譯成 `text/chapter1.zh-TW.tsv`（3,299 個不重複 key；228 個開發佔位 `bugbugbug.` 不譯）。進入第一章辦公室後，章節標題、地點列（含插槽範本，例：檢視畫像）、旁白、物品欄描述、對話選單與子選單、角色回答都以中文顯示（`ch1-zh3`、`ex-scan9`、`ex-belov6`）；片語表之外的 216 個介面與選單片語也已萃取翻譯。動作選單、物品選單、游標動詞與開場字卡是圖片，以覆繪方式中文化（docs/spec/005）。多片語區塊的動作選單、框內訊息與物品插槽組句未完成（worklist `composite-blocks`）。章節片語的 key 是比對鍵的 FNV-1a 64 雜湊，英文原文不進版控。SQX 解壓工具已驗證（docs/re/003）。
- 第二到四章片語已全部翻譯：`text/chapter2.zh-TW.tsv`（CHAP2A，2,501 key）、`chapter3`（CHAP3U，298 key）、`chapter4`（CHAP4U，768 key）；說明影片第二到四章提示（THA2–THA4）在 `text/help-ch234.zh-TW.tsv`（172 key）。各章片語彼此不重疊。以 `kgb-run --chapter N --cycles 1000` 從各章開頭抽樣，除時鐘外的文字事件全部有譯文、畫面顯示中文（docs/re/006，ch2-d、ch3-a、ch4-a）；比對鍵命中也證實各章用的是 CHAP2A、CHAP3U、CHAP4U。
- 片頭名單（docs/spec/006）：名單程式逐行畫字前接手（`8a07`），跳過英文；職稱以中文、真人姓名與公司名以原文畫在覆繪層，跟著原版每頁的淡入淡出（credits-zh2）。譯文在 `text/credits.zh-TW.tsv`。片尾名單走同一路徑，以 `kgb-run --end-credits` 從腳本的片尾路徑觸發實測，演員表的角色名畫在頭像旁（endcred-4、endcred-5）。
- 兩個中文版本：繁中（台灣用語）為翻譯產物；簡中 `text/*.zh-CN.tsv` 由 `kgb-zhcn` 以 OpenCC tw2sp 加譯名表簡中欄從繁中產生，`kgb-zhcn --check` 驗證產物與來源一致。
- 本機完整版（v1.1.1，中文名《KGB 陰謀》）：`tools/package.sh with-data all` 建出 Linux AppImage、Windows zip、macOS universal .app，內含原版，集中放在 `dist-all/`。Linux 版在 Xvfb 冷開機，繁中與簡中語系都自動選對語言並顯示中文字幕（smoke-linux1、smoke-linux-cn；v1.1.0 為 smoke-linux-v110）；Windows、macOS 只做了架構與內容的靜態檢查，沒有實機。推廣影片 `tools/promo.sh` 分繁中、簡中兩版（90 秒，含 40 秒實機片段，配樂為模擬器錄下的片頭名單 AdLib 音樂）。
- 聲音：FerroGolem 合成 AdLib（Nuked OPL3）並混入 SB 樣本，`kgb-play` 由前端輸出，`kgb-run --wav` 錄成 WAV（FerroGolem docs/spec/007）。開場的主角獨白（INTRO 第 2 段）原版就沒有語音，萬尼亞叔叔的獨白（第 0 段）與各章說明影片由 SB 播出語音（docs/re/007）。v1.1.1 的 Linux AppImage 在 PulseAudio 虛擬喇叭上錄到配樂與語音（`tools/smoke-audio.sh`，smoke-audio-v111）。抽樣的實跑裡 SB 請求都來自影片，沒看到獨立的數位音效。
- 可開視窗遊玩：`tools/rust.sh play`（FerroGolem GUI 前端）。工具列可選繁中、简中、英文原版，預設依 `LC_ALL`、`LC_MESSAGES`、`LANG`。Xvfb 無頭驗證：繁中字幕（`gui-k1`、`gui-k2`）、簡中字幕與 SC 字形（`gui-cn1`）、英文原版由原版照畫（`gui-en1`）。執行中點選切換語言尚未以 GUI 操作驗證。時脈改為 cycles 20000 後開場以正常速度播放。
- 對拍基準 dosgolem-rust 分支 `kgb-oracle` 已跑出完整開場，6 句字幕與攔到的事件逐字相同（FerroGolem docs/re/001）。開場字卡「Moscow, August 1991...」不經文字 routine，推測是語系圖檔（worklist `image-localization`）。
- 未完成項見 `docs/worklist.json`，以 `tools/worklist.py verify` 檢查。

## 術語

| 術語 | 意思 |
|---|---|
| FerroGolem | 自有 clean-room RRSAL Rust DOS 模擬器，另一個 repo |
| 對拍基準 | `~/cht/dosgolem-rust` 分支 `kgb-oracle`（rust-dos） |
| 文字事件 | 被接手的一次繪字呼叫：原文、呼叫點、座標、字型狀態 |
| 比對鍵 | 原版英文原文；排版斷行的 0 還原成空白，`^` 保留，去尾端 0 與空白 |
| 覆繪層 | 放大 3 倍（960×600）合成的中文文字層 |
| 字幕 | 開場在畫面底部（y＝181 起）的英文句子 |
