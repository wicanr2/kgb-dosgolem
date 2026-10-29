# 目前狀態

核對日期：2026-10-29。

## 現況

- 原版光碟已解到 `workplace/disc/`（docs/re/001）。
- Go dosgolem `f79eab6` 可把 KGB 跑到 `LOGOS.BGF` 片頭動畫，作為對拍基準（docs/re/002）。
- 自有核心 `crates/dosgolem-core` 已建立，種子取自 xaika `dd3f8b8`。KGB 在第 398,768 步停在 EMS `int 67h AH=41h`。
- 未完成項見 `docs/worklist.json`，以 `tools/worklist.py verify` 檢查。

## 術語

| 術語 | 意思 |
|---|---|
| Go 基準 | Go dosgolem `f79eab6` 的 `cmd/probe`，唯讀建置於 `workplace/bin/` |
| 自有核心 | 本庫 `crates/dosgolem-core`，RRSAL |
| 種子 | 從 xaika `src/native` 複製來的初始程式 |
| 停點 | 自有核心遇到不支援的指令或服務時停下的 CS:IP 與步數 |
| 比對鍵 | 辨識原版字串用的英文原文 |
