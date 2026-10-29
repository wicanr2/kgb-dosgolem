# 001 原版光碟清單

## 輸入

| 檔 | SHA-256 |
|---|---|
| `KGB (1992).zip` | `a12481886606511e8a28f88a41523d0a2334032cf413b6e595020dd79e5ed3fc` |
| `KGB/cd/KGB.iso`（219,996,160 bytes，107,420 扇區） | `10c1e4a28546d687bf8feb580b0460755fdbad4c05a939a31ba7b907e27d48a3` |
| `KGB/CONSP.BAT`（106 bytes） | `f0648fe29407834862488adc23465c7ad41ca4e637014e39d033f4c789d3b52d` |
| `KGB/CSP.CFG`（6 bytes：`01 00 00 00 02 00`） | `263d99957c7b574c63a265b32da7fff8b8ad831828946bfbff650d7074dd9198` |

ISO9660 卷名 `CONSPIRACY`，根目錄 184 個檔，沒有子目錄。映像只有資料軌，沒有 CD 音軌。

## 重產

```sh
docker run --rm --network none -u "$(id -u):$(id -g)" \
  -v "$PWD/KGB (1992).zip":/in/kgb.zip:ro -v "$PWD/tools":/tools:ro \
  -v "$PWD/workplace":/out --entrypoint python3 dosgolem-rust-sc:1.99.1-r1 \
  -I /tools/extract_disc.py /in/kgb.zip /out/disc
```

（另帶 AGENTS.md 第 9 節的資源限制。）工具先驗 zip 與 ISO 的 SHA-256，不符就停止。輸出 `workplace/disc/cd/` 與 `workplace/disc/manifest.json`，後者列每個檔的 LBA、大小與 SHA-256。逐檔雜湊以 manifest 為準，本文不重抄。

## 已觀察

- `CONSP.BAT`：`D:` 後執行 `CSPPRG AMR ADP220 SBP2227 EMS 386 SAF WRIC:\KGB\`。已證實（檔案內容）。
- `CSPPRG.EXE`：MZ，31,654 bytes，`e_lfarlc` 0x1c。字串片段破碎，判為壓縮過的執行檔（強推論）。啟動時讀 `C:\KGB\CSP.CFG`（已證實，docs/re/002）。
- `CONSP.EXE`（3,660 bytes）、`INSTALL.EXE`、`INSTDOS.EXE` 是啟動與安裝程式，字串含 `CONSPIRACY-CD launcher V3.6` 與 `(c) 1993`。
- 語系相關檔：`CHAP{1..4}{A,D,F,S,U}` 各有 `.PHZ`、`.SQX`、`.WRQ`（`CHAP2` 沒有 U）；`TH{A,D,F,S}{1..4,H,I,T}.SQX`；`TCD{1,2}{A,D,F,S}.SCR`；`LANG0..4.SQX`。字母與語言的對應是假說。
- 影片：`HELP.BGF` 179,493,026 bytes、`INTRO.BGF` 34,525,402 bytes、`LOGOS.BGF` 1,759,728 bytes。格式未知。
- `README.DOC`：CD 傳輸率、IRQ 與 MSCDEX 緩衝的安裝說明，提到 EMS／XMS「350k 到 1M」可加速。
