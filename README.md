# dosgolem-rs

自有的 Rust DOS 執行器，第一個目標是讓 Cryo《KGB》CD 版（Conspiracy）在自有核心上執行，再以執行期覆繪做繁體中文化。

目前只完成光碟取證、Go 對拍基準與核心種子。KGB 在自有核心上停在 EMS 服務，還不能玩。現況見 [CONTEXT.md](CONTEXT.md)，未完成項見 [docs/worklist.json](docs/worklist.json)。

使用者必須自備合法原版。本庫不含遊戲檔、影片、音訊、字型或截圖。

## 入口

```sh
tools/rust.sh fetch                 # 只有這一步開網路
tools/rust.sh build
tools/rust.sh test
tools/rust.sh probe <輸出名> <步數>  # 需先解出 workplace/disc（docs/re/001）
```

所有建置與執行都在 Docker。

- [工作契約](AGENTS.md)、[目前狀態](CONTEXT.md)、[歷程](WORKLOG.md)
- [證據索引](docs/re/README.md)、[核心移植規格](docs/spec/001-native-core-port.md)

## 授權

採 RRSAL-1.0（復古重製 source-available 授權條款），見 [LICENSE](LICENSE)。核心種子移植自 [dosgolem](https://github.com/wicanr2/dosgolem) 與 dosgolem-rust-xaika，來源與雜湊見 `external/`。本專案與 Cryo Interactive、Virgin Games 及其權利繼承者沒有隸屬或授權關係。
