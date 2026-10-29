# 003 Sound Blaster、8237 DMA 與硬體中斷

狀態：READY（§2 到 §4）；自動初始化 DMA、16 位元 DMA 與聲音輸出為 DRAFT。

## 1. 證據

- 命令列尾 `SBP2227`：KGB 只掛 `int 0Fh`（`5156:03AD`），埠寫入落在 220h 起。判讀為「SB、220h、IRQ 7」（強推論，`workplace/runs/rs-r7`）。
- 偵測序列（自有核心，`workplace/runs/rs-r6`）：DSP reset（226h 寫 1、0，讀 22Ah 得 AAh）、`E0h` 識別、`E1h` 版本、8237 通道 1 遮罩、模式 48h（單次、讀記憶體）、頁 5、位址 17F1h、計數 0、解除遮罩、`40h` 時間常數 100、`14h` 長度 1 位元組。之後等 IRQ 7。
- Go 基準：real-mode 的 `soundBlaster.Out8` 對未接 DMA 的命令回 false，`Machine.Out8` 不報錯就往下走，所以 Go 那次執行沒有 DMA 傳輸也沒有 IRQ 7，KGB 的偵測是逾時結束。本庫刻意不照做（見 §5）。

## 2. DSP（220h）

移植 Go `internal/machine/sb_dsp.go`：reset 握手、mixer 32h／33h／80h–82h、`40h`、`41h`、`48h`、`14h`、`1Ch`、`B0h`、`C6h`、`D0h`／`D4h`、`D1h`／`D3h`、`D8h`、`E1h`。另依 DOSBox-X `src/hardware/sblaster.cpp`（`case 0xe0`）補 `E0h`：下一個資料位元組的反相放進讀取佇列。

需要 DMA 的命令只有在核心接上 DMA 時接受。目前接受 `14h`（8 位元單次）；`1Ch`、`B0h`、`C6h`、`D0h`、`D4h` 仍回不支援，核心停在原處。

## 3. 8237 第一顆控制器

移植 Go `internal/machine/dma8237.go`：埠 00h–07h 位址與計數（高低位元組正反器）、0Ah 單一遮罩、0Bh 模式、0Ch 清正反器、0Dh 主清除、0Eh 清全部遮罩、0Fh 寫全部遮罩、頁暫存器 87h／83h／81h／82h。

傳輸（本庫新增，依 8237 資料表與 DOSBox-X `src/hardware/dma.cpp` 的單次模式語意）：

- SB 接受 `14h` 時，若通道 1 未遮罩，開始傳輸 N＝長度＋1 個位元組。
- 每個位元組間隔 `CYCLES_PER_SECOND × 分母 ÷ 分子` 個 CPU 週期。`CYCLES_PER_SECOND` 取 4 × 1,193,182，與種子 IRQ0 的「PIT 分頻 × 4 週期」一致。
- 每傳一個位元組：讀線性位址 `頁 × 10000h + 目前位址`，目前位址加 1，目前計數減 1。
- 傳完 N 個：SB `irq_pending` 設為真，8259 的 IRQ 7 掛起。

## 4. 8259（主片）

- 埠 21h 寫入保存 IMR，讀回 IMR。埠 20h 的命令（含 EOI）接受、不追蹤 in-service，與 Go real-mode 相同。
- IRQ0：到期掛起；`IF` 為 1 且 IMR bit0 為 0 才送 `int 08h`。種子原本在 `IF` 為 0 時丟掉該次 tick，改照 Go `machine.go` 的掛起語意。
- IRQ7：掛起後，`IF` 為 1 且 IMR bit7 為 0 時送 `int 0Fh`。同一步只送一個硬體中斷，IRQ0 優先。

## 5. 已知差異

| 項目 | 類別 | 說明 |
|---|---|---|
| SB DMA 與 IRQ 7 | 刻意不照 Go 基準 | Go 沒有傳輸與中斷，KGB 偵測逾時。本庫照真機行為完成偵測，開檔順序可能與 Go 基準在此之後分岔，以 DOSBox-X 取證為準 |
| in-service 追蹤 | remake-owned | 未實作 ISR 與 EOI 優先序；遇到依賴它的程式再補 |
| 聲音輸出 | remake-owned | 傳出的位元組只記錄，尚未混音播放 |

## 6. 驗收

- 單元測試：DSP 握手、E0h、時間常數、DMA 未接時拒絕（負對照）；8237 程式序列與未知埠拒絕；單次傳輸位元組數、位址遞增、結束後 IRQ 7 掛起；IMR 遮罩時不送（負對照）。
- `tools/rust.sh probe`：KGB 越過 SB 偵測，`int 0Fh` 至少被送一次。
