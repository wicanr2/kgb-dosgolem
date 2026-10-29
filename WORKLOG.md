# 工作歷程

## 2026-10-29

- 建立工作契約 AGENTS.md，git init，建立私人 repo `wicanr2/dosgolem-rs` 並推送首個 commit `4bc99ad`（使用者授權）。
- 以 `tools/extract_disc.py` 解出光碟，184 檔，寫 manifest。
- 唯讀建置 Go dosgolem `f79eab6` 的 `cmd/probe`。KGB 啟動器需要 `C:\KGB\CSP.CFG`，提供 zip 內同名檔後跑到 `LOGOS.BGF` 片頭。
- 複製 xaika `src/native`（`dd3f8b8`）為 `crates/dosgolem-core`，加 `load_with_tail`。LICENSE 改用 RRSAL-1.0 範本，作品名填 dosgolem-rs。
- 寫 `dgrs-probe` 與 `tools/rust.sh`。自有核心停在第 398,768 步 EMS `int 67h AH=41h`。
- 建 `docs/worklist.json` 與 `tools/worklist.py`（selftest 含正反對照）。
- 依停點逐項補核心：EMS（Go 移植）、SB DSP 與 mixer、DSP E0h（依 DOSBox-X）、8237、單次 DMA 與 IRQ 7、8259 IMR、IRQ0 改為掛起語意、軟碟 DOR、mode 13h。停點紀錄在 docs/re/002。
- Go 基準對未支援的埠寫入只記錄不停，所以它的 SB 偵測是逾時結束；本庫照真機完成偵測，列為刻意不照做（docs/spec/003 §5）。
- 自有核心 50M 步跑到 INTRO.BGF 開場與英文字幕，開檔順序前 14 檔與 Go 相同。
- 發現時脈比例差異：本庫每 tick 約 3,200 道指令，Go 約 58,000 道。新增 worklist `cpu-clock`。
- 測試期間一次 transfer 測試失敗是測試本身沒取走 reset 的 AAh，已修測試。
