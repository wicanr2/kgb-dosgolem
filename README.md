# dosgolem-rs：KGB 中文化

把 Cryo《KGB》CD 版（Conspiracy）在執行期中文化，分繁中（台灣用語）與簡中（大陸用語）兩個版本。遊戲跑在自有的 Rust DOS 模擬器 [FerroGolem](../ferrogolem)（另一個 repo）上，本庫接手遊戲的文字 routine，跳過英文繪字，把中文畫在放大 3 倍的覆繪層。

目前開場字幕、第一到四章片語與說明影片字幕已有中文，可開視窗遊玩並在工具列切換繁中、简中、英文原版；第一章已實機驗證主要介面，第二章起尚未實機走過，聲音尚未輸出。現況見 [CONTEXT.md](CONTEXT.md)，未完成項見 [docs/worklist.json](docs/worklist.json)。

使用者必須自備合法原版。本庫不含遊戲檔、影片、音訊、字型或截圖。

## 入口

```sh
tools/rust.sh fetch                 # 只有這一步開網路
tools/rust.sh build
tools/rust.sh test
tools/rust.sh kgb <輸出名> <步數> [--shots N]
tools/rust.sh play [--lang zh-TW|zh-CN|en]   # 在 X11 桌面開視窗玩；預設語言依系統語系
```

需要 `~/cht/ferrogolem` 與主機的 Noto Sans CJK 字型（`NotoSansCJK-Medium.ttc`）；先依 [docs/re/001](docs/re/001-disc-inventory.md) 解出光碟。所有建置與執行都在 Docker。

- [工作契約](AGENTS.md)、[目前狀態](CONTEXT.md)、[歷程](WORKLOG.md)
- [證據索引](docs/re/README.md)、[文字 routine 接手規格](docs/spec/004-kgb-text-takeover.md)

## 授權

採 RRSAL-1.0（復古重製 source-available 授權條款），見 [LICENSE](LICENSE)。本專案與 Cryo Interactive、Virgin Games 及其權利繼承者沒有隸屬或授權關係。
