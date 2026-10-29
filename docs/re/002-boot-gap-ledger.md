# 002 KGB 啟動缺口對照

同一份輸入（docs/re/001 的 `workplace/disc`）、同一命令列尾 ` AMR ADP220 SBP2227 EMS 386 SAF WRIC:\KGB\`，`C:\KGB\CSP.CFG` 由 zip 內的同名檔提供。

## Go dosgolem 基準

- 版本：`~/cht/dosgolem` commit `f79eab6db8c64a524a06535e53e5d32b0bbcf7c8`，工作樹乾淨，以唯讀掛載建置 `cmd/probe`。
- 參數：`-cpu` 預設（386）、`-cdrom D:<曲目表>`（單一資料軌，leadout 107420）、`-sound-blaster`、`-scratch`。
- 結果（50,000,000 步，`workplace/runs/go-r2/out.txt`）：
  - #398,738 `AH=4Ah` 調整 PSP 區塊；#398,816 EMS 配置 handle 1、32 頁。
  - #476,822 切到 mode 13h。設 `int 09h`、`int 08h`、`int 24h`。PIT 通道 0 分頻 5957。
  - 開檔順序：`KGBADP.SQX KGBSDP.SQX KGBMCGA.SQX LANG0.SQX POLB.ANC POLS.ANC POLH.ANC SPRA.ANC TAB.PAL TAB.SQX THAH.SQX CSP.CFG LOGOS.BGF MORNING.SQX`，300M 步時另開 `CRYOMUS.SQX`。
  - `int 33h`：0000×1、0003（輪詢）、0004×2、0007×1、0008×1。
  - 寫過的埠：020 021 040 043 224 225 226 22C 388 389 3C8 3C9 3F2。
  - 300M 步時畫面為 `LOGOS.BGF` 片頭動畫（`workplace/runs/go-r3/screen.png`）。
- 推論等級：開檔與服務紀錄已證實（單次執行）；`LANG0` 對應英文是假說。

## 自有核心

- 版本：本庫 `crates/dosgolem-core`（種子 dosgolem-rust-xaika `dd3f8b8`，加命令列尾），`tools/rust.sh probe rs-r1 50000000`。
- 結果：第 398,768 步停在 `188b:3580`，bytes `cd 67 80 fc 80 f5 c3 b4`，`AX=4102`，錯誤 `unsupported interrupt 67`。之前用過 `int 10h AH=0Fh`、`int 21h` 19／36／48×2／4A。
- 解讀：`AH=41h` 是 EMS「取 page frame 段」。與 Go 基準 #398,816 的 EMS 配置同一段落（強推論：步數差 48 來自兩邊計步與服務實作不同，未逐條對位）。

## 缺口順序

依 Go 基準的紀錄，KGB 到片頭前會用到：EMS、滑鼠 `int 33h`、mode 13h 與 DAC、PIT 重設分頻、IRQ0／IRQ1 改向、Sound Blaster DSP（220h）與 OPL（388h）、軟碟控制埠 3F2。MSCDEX 是否被查詢尚未量。每補一項重跑本對照，追加新的停點。
