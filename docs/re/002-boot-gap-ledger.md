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

## 停點紀錄（自有核心）

| 輸出 | 停在 | 原因 | 處置 |
|---|---|---|---|
| rs-r1 | #398,768 `188b:3580` | `int 67h AX=4102` | 移植 EMS（docs/spec/002） |
| rs-r2 | #473,195 `5156:0365` | 寫 226h（DSP reset） | 移植 DSP（docs/spec/003） |
| rs-r3 | #473,237 `5156:0311` | 22Ch 寫 E0h（DSP 識別） | 依 DOSBox-X 補 E0h |
| rs-r4 | #473,386 `5156:045d` | 寫 0Ah（8237 遮罩） | 移植 8237 狀態 |
| rs-r6 | #473,460 `5156:0345` | DSP `14h` 要求 1 位元組 DMA | 補單次 DMA、IRQ 7、8259 IMR |
| rs-r9 | #475,138 `188b:63b2` | 寫 3F2h 值 0Ch | 保存軟碟 DOR |
| rs-r10 | #482,534 `51e0:0aa2` | `int 10h` 設 mode 13h | 補 mode 13h（Go setMode13、Unchained256） |
| rs-r11 | #549,763 `50bb:096d` | 寫 225h 值 CCh（SB Pro mixer） | mixer 改為一般暫存器檔 |
| rs-r13 | 跑滿 50,000,000 步 | 無 | 見下節 |

## 自有核心 50M 步（rs-r13）

- 開檔順序前 14 檔與 Go 基準相同，步數相差 23 以內（例：`KGBADP.SQX` #398,863，Go 讀取於 #398,886）。
- 之後：#12,188,271 `CRYOMUS.SQX`、#35,602,634 `INTRO.BGF`、#35,602,729 `TCD1A.SCR`、#38,118,024 `THAT.SQX`、#38,118,143 `KURSK.SQX`。
- 畫面：開場動畫，底部遊戲自繪英文字幕 `Maksim Mikhailovich Rukov is my real name but I use other names,`。
- `int 33h` 只呼叫 AX=0 一次，種子回「沒有滑鼠」；Go 基準有滑鼠並持續輪詢 0003h。
- IRQ 7 送達 1 次（SB 偵測）。

兩邊進度不同的原因（強推論）：種子每次 IRQ0 間隔為 PIT 分頻 × 4 週期，50M 步約 15,700 次 tick；Go 基準同步數送 857 次。KGB 以 tick 計時，本庫的遊戲時間相對指令數快約 18 倍。另一個差異是滑鼠有無。兩者未分離量測。
