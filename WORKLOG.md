# 工作歷程

## 2026-10-29

- 建立工作契約 AGENTS.md，git init，建立私人 repo `wicanr2/dosgolem-rs` 並推送首個 commit `4bc99ad`（使用者授權）。
- 以 `tools/extract_disc.py` 解出光碟，184 檔，寫 manifest。
- 唯讀建置 Go dosgolem `f79eab6` 的 `cmd/probe`。KGB 啟動器需要 `C:\KGB\CSP.CFG`，提供 zip 內同名檔後跑到 `LOGOS.BGF` 片頭。
- 複製 xaika `src/native`（`dd3f8b8`）為 `crates/dosgolem-core`，加 `load_with_tail`。LICENSE 改用 RRSAL-1.0 範本，作品名填 dosgolem-rs。
- 寫 `dgrs-probe` 與 `tools/rust.sh`。自有核心停在第 398,768 步 EMS `int 67h AH=41h`。
- 建 `docs/worklist.json` 與 `tools/worklist.py`（selftest 含正反對照）。
