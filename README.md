# dosgolem-rs：KGB 繁體中文化

把 Cryo《KGB》CD 版（Conspiracy）在執行期繁體中文化。遊戲跑在自有的 Rust DOS 模擬器 [FerroGolem](../ferrogolem)（另一個 repo）上，本庫接手遊戲的文字 routine，跳過英文繪字，把中文畫在放大 3 倍的覆繪層。

目前開場字幕已能攔截並由覆繪層顯示，還沒有譯文，也沒有 GUI。現況見 [CONTEXT.md](CONTEXT.md)，未完成項見 [docs/worklist.json](docs/worklist.json)。

使用者必須自備合法原版。本庫不含遊戲檔、影片、音訊、字型或截圖。

## 入口

```sh
tools/rust.sh fetch                 # 只有這一步開網路
tools/rust.sh build
tools/rust.sh test
tools/rust.sh kgb <輸出名> <步數> [--shots N]
```

需要 `~/cht/ferrogolem` 與主機的 Noto Sans CJK TC 字型；先依 [docs/re/001](docs/re/001-disc-inventory.md) 解出光碟。所有建置與執行都在 Docker。

- [工作契約](AGENTS.md)、[目前狀態](CONTEXT.md)、[歷程](WORKLOG.md)
- [證據索引](docs/re/README.md)、[文字 routine 接手規格](docs/spec/004-kgb-text-takeover.md)

## 授權

採 RRSAL-1.0（復古重製 source-available 授權條款），見 [LICENSE](LICENSE)。本專案與 Cryo Interactive、Virgin Games 及其權利繼承者沒有隸屬或授權關係。
