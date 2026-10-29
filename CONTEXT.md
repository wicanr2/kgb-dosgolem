# 目前狀態

核對日期：2026-10-29。

## 現況

- 原版光碟已解到 `workplace/disc/`（docs/re/001）。
- Go dosgolem `f79eab6` 可把 KGB 跑到 `LOGOS.BGF` 片頭，作為對拍基準（docs/re/002）。
- 自有核心 `crates/dosgolem-core`（種子 xaika `dd3f8b8`）已補 EMS、SB DSP／mixer、8237 單次 DMA、IRQ 7、8259 IMR、mode 13h。KGB 在 50M 步內跑到 `INTRO.BGF` 開場，畫面底部有遊戲自繪的英文字幕，沒有停點（docs/re/002 停點紀錄）。
- 尚未有 GUI，只能用 `dgrs-probe` 無頭執行並存最後一格畫面。滑鼠回報不存在。
- 本庫與 Go 基準的 CPU 時脈比例不同（約 18 倍），步數無法直接對位。
- 未完成項見 `docs/worklist.json`，以 `tools/worklist.py verify` 檢查。

## 術語

| 術語 | 意思 |
|---|---|
| Go 基準 | Go dosgolem `f79eab6` 的 `cmd/probe`，唯讀建置於 `workplace/bin/` |
| 自有核心 | 本庫 `crates/dosgolem-core`，RRSAL |
| 種子 | 從 xaika `src/native` 複製來的初始程式 |
| 停點 | 自有核心遇到不支援的指令或服務時停下的 CS:IP 與步數 |
| 比對鍵 | 辨識原版字串用的英文原文 |
| 字幕 | 遊戲在 mode 13h 畫面底部自繪的英文句子，中文化的首要目標之一 |
