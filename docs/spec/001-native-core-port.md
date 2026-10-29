# 001 自有核心的移植來源與補服務順序

狀態：READY（移植政策）；各服務的行為契約另立規格。

## 範圍

- 種子：dosgolem-rust-xaika `src/native`，commit `dd3f8b88aa4f0d811bc623da32fccb91dc455058`，逐檔雜湊見 `external/xaika-native/source.json`。種子本身移植自 Go dosgolem `f79eab6`（`external/rrsal-go/source.json`）。
- 補服務的依據依序為：Go dosgolem `f79eab6` 的 READY 規格與程式、DOSBox-X 原始碼（`~/cht/DOSBox-X-MCP-Debugger/dosbox-src/`）、公開硬體與 DOS 規格。rust-dos 只當行為對照，不讀其程式碼當實作依據。
- 不在範圍：GUI、KGB 專屬攔截、中文覆繪。

## 規則

- 一次補一項服務，順序照 docs/re/002 的缺口對照。每項先寫出 Go 規格編號與本庫的差異，再移植。
- 移植的 Go 檔記在 `external/rrsal-go/source.json`（路徑與 SHA-256）。
- 未知指令或服務照種子行為回報位置後停止，不以空回傳繞過。
- 每補一項：單元測試（含負對照）、`tools/rust.sh probe` 重跑並在 docs/re/002 追加停點。

## 本庫對種子的改動

| 改動 | 理由 | 依據 |
|---|---|---|
| `Machine::load_with_tail` 寫 PSP 80h 命令列尾 | KGB 啟動器靠命令列尾選裝置與寫入路徑 | DOS PSP 格式（80h 長度、81h 起內容、0Dh 結尾） |
| EMS | KGB 需要 | docs/spec/002-ems.md |
| SB DSP、mixer、8237、單次 DMA、IRQ 7、8259 IMR、IRQ0 掛起 | KGB 偵測 SB | docs/spec/003-sound-blaster.md |
| 軟碟 DOR 3F2h 只保存 | KGB 寫 0Ch 關馬達 | 8272 DOR 公開格式；Go 對未知埠只記錄 |
| mode 13h 鏈結與非鏈結 | KGB 畫面模式 | Go `bda.go` SetVideoMode、`vga.go` setMode13、Unchained256 |
| 開檔紀錄 `file_log` | 與 Go 基準比對開檔順序 | 觀測用，不改行為 |

## 驗收

- KGB 在自有核心上跑到 `LOGOS.BGF` 片頭，開檔順序與 Go 基準相同，同步數畫面 indexed 與色盤雜湊相同。
- 未達成前，本規格之下的每項服務以各自規格驗收，不宣稱 KGB 可玩。
