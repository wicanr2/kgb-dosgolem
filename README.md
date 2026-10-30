# KGB 陰謀（kgb-dosgolem）

《KGB 陰謀》是 Cryo《KGB》CD 版（片名 *Conspiracy*，光碟卷名 `CONSPIRACY`）的繁體中文與簡體中文化，簡中名《KGB 阴谋》。這款遊戲沒有官方中文名，中文名取自 CD 版的副名。遊戲跑在自有的 Rust DOS 模擬器 [FerroGolem](https://github.com/wicanr2/ferrogolem) 上；本庫攔下遊戲畫字的程式，跳過英文，把中文畫在放大 3 倍的高解析覆繪層。原版檔案一個位元組都不改。

開場字幕、四章對白與旁白、說明影片字幕、選單、游標、片頭與片尾名單都已有中文，四章都在遊戲裡抽樣確認過。工具列可以隨時切換繁中、简中與英文原版。聲音還沒有輸出。

![第一章：KGB P 處辦公室，進場旁白](docs/images/ch1-office-narration.png)

*第一章開場，KGB P 處的辦公室。旁白、地點列「你的辦公室」與游標動詞「檢視」都是覆繪層上的中文。截圖為 `kgb-run` 實跑，cycles 1000，繁中。*

## 緣起

bilibili 上的夥伴索菲婭（ソフィア）希望我中文化一款冷門的經典遊戲，這個專案從那裡開始。

## 故事

以下取自遊戲的開場旁白、章節標題與場景文字；P 處的職責與案件起因另參考 [Wikipedia](https://en.wikipedia.org/wiki/KGB_(video_game))。

主角馬克西姆·米哈伊洛維奇·魯科夫原本是特種部隊（Spetsnaz）軍官。特種部隊隸屬軍事情報局格魯烏（GRU），而格魯烏又受 KGB 管轄，所以當他收到「邀請」，要他加入莫斯科 KGB 第二總局的 P 處時，根本沒有拒絕的餘地。P 處負責清查 KGB 內部的腐敗；他報到時，一名轉做私家偵探的前探員剛被發現遭到謀殺，魯科夫被派去調查背後可能的內部腐敗。

魯科夫的父母在他小時候死於塔吉克的一場汽車炸彈。父親是格魯烏軍官，生前痛恨 KGB 的一切；同樣當過格魯烏軍官的萬尼亞叔叔則提醒他：盡忠職守，但要提防背後。CD 版加入了 Donald Sutherland 飾演的父親，他在說明影片中以魯科夫記憶裡的模樣現身給建議，最後留下一句話：「在這一切腐敗底下，埋藏著千百萬正直的人曾經相信的理想。他們也許是傻子，但他們有權知道真相。」

| 章 | 時間與地點 | 開場 |
|---|---|---|
| 一 | 莫斯科，1991 年 8 月 14 日，KGB P 處 | 調到 P 處的第一天，上級是沃夫洛夫少校，同事別洛夫上尉正在打報告 |
| 二 | 列寧格勒，8 月 16 日星期五，戈斯季尼察飯店 | 下午 1 點，搭了一趟不舒服的火車，抵達飯店 205 號房 |
| 三 | 列寧格勒，8 月 17 日星期六，碼頭 | 19 號碼頭 |
| 四 | 列寧格勒，8 月 18 日星期日，碼頭 | 19 號碼頭 |

開場旁白說，這段冒險取材自 1991 年 8 月的一連串事件，而那場政變最後失敗了；魯科夫還說，他喜歡想像那多少有他的功勞。

![KGB 陰謀主要人物](docs/images/characters.png)

*人物總覽。身分取自遊戲開場旁白、場景文字、片尾演員表與譯名表備註；查不到身分的只列名字與出場地點，出場章節以譯文所在的章節檔判斷。*

## 時代背景

遊戲在 1992 年發行，故事卻停在它發行前一年、蘇聯最後的那個夏天。下面整理遊戲沒有明說、但玩家會想知道的歷史。

**KGB 是什麼。** KGB 是俄文 Комитет государственной безопасности 的縮寫，意思是「國家安全委員會」，大陸常音譯為「克格勃」。它在 1954 年 3 月 13 日成立，前身是史達林時代的 MGB 與 MVD，兼管對外情報、反情報與國內的政治安全，直到 1991 年 12 月 3 日正式解散，職能由俄羅斯的對外情報局（SVR）與後來的聯邦安全局（FSB）承接。遊戲裡的「第二總局」是 KGB 負責國內反情報與內部安全的部門；主角原屬的格魯烏則是蘇軍的情報機關，與 KGB 是兩個系統。

**1991 年的蘇聯。** 戈巴契夫在 1980 年代中期推動改革重建（Perestroika）與開放，遊戲裡的角色也常提到「改革重建」。到了 1991 年，聯盟內部的分離聲浪越來越大。8 月 18 日，一群強硬派高官前往克里米亞的福羅斯別墅，要求正在度假的戈巴契夫宣布全國緊急狀態，遭拒後將他軟禁。隔天 8 月 19 日，他們以「國家緊急狀態委員會」的名義宣布接管政權，成員包括副總統亞納耶夫、國防部長亞佐夫，以及 KGB 主席克留奇科夫。葉爾欽在俄羅斯議會大樓（白宮）前帶頭反抗，軍隊與 KGB 內部也拒絕全面配合，政變在 8 月 22 日前後瓦解。四個月後，1991 年 12 月 26 日蘇聯解體。

**列寧格勒還是列寧格勒。** 1991 年 6 月 12 日，列寧格勒市民以公投支持恢復帝俄時代的舊名「聖彼得堡」，但公投只是諮詢性質，正式改名要等到 9 月 6 日。所以遊戲在 8 月 16 日到 18 日的列寧格勒，名字還沒改；第二章裡有人問魯科夫「你比較喜歡這座城市叫聖彼得堡嗎？」，答案是「聖彼得堡。對觀光比較好。」

**遊戲與史實的位置。** 遊戲四章落在 8 月 14 日到 18 日，最後一章正好是戈巴契夫被軟禁的那一天、政變爆發的前一天。KGB 主席本人參與政變，而遊戲讓玩家扮演一名清查 KGB 內部腐敗的探員，這是本作劇情張力的來源。

![KGB 陰謀歷史與遊戲時間線](docs/images/timeline.png)

*時間線。金色為遊戲四章的日期，白色為史實。*

出處：[KGB（Wikipedia）](https://en.wikipedia.org/wiki/KGB)、[1991 Soviet coup attempt（Wikipedia）](https://en.wikipedia.org/wiki/1991_Soviet_coup_attempt)、[Meduza：1991 年政變問答](https://meduza.io/en/feature/2021/08/20/questions-about-the-failed-1991-soviet-coup-that-you-re-too-embarrassed-to-ask)、[俄羅斯總統圖書館：列寧格勒恢復彼得之名](https://www.prlib.ru/en/node/619516)、[UPI 1991-09-06](https://www.upi.com/Archives/1991/09/06/Leningrad-renaming-moves-closer/2891684129600/)、[KGB（遊戲，Wikipedia）](https://en.wikipedia.org/wiki/KGB_(video_game))、[Hardcore Gaming 101](https://www.hardcoregaming101.net/kgb-conspiracy-starring-donald-sutherland/)。

## 畫面

| | |
|---|---|
| ![對話選單](docs/images/ch1-dialog-menu.png) | ![動作選單](docs/images/ch1-action-menu.png) |
| 和別洛夫交談的「詢問」子選單，選項依原版英文行的中點置中。 | 右鍵動作選單。按鈕上的英文是圖塊，覆繪時先用原版底色蓋掉再寫中文。 |
| ![第二章 205 號房](docs/images/ch2-room205.png) | ![第四章碼頭](docs/images/ch4-docks.png) |
| 第二章，列寧格勒飯店 205 號房。 | 第四章，19 號碼頭，地點列「前往棧橋。」 |
| ![重播時的框內訊息](docs/images/ch1-replay-box.png) | ![片頭名單](docs/images/credits-opening.png) |
| 按倒帶鈕後的重播模式，框內訊息與回答都是中文。 | 片頭名單，職稱譯成中文，人名保留原文，跟著原版每頁的淡入淡出。 |
| ![片尾演員表](docs/images/credits-cast.png) | |
| 片尾演員表，角色名畫在頭像另一側。這張用取證選項從第一章觸發，背景與正常破關不同。 | |

截圖都是 `kgb-run` 無頭實跑的 3 倍覆繪畫面（繁中）。第二、四章與片尾用取證選項 `--chapter`、`--end-credits` 直接進入，只改暫存器，見 [docs/re/006](docs/re/006-chapter-start.md) 與 [docs/spec/006](docs/spec/006-credits.md)。

## 中文化的做法

KGB 把英文畫在 320×200 的畫面上。直接改原版字串會碰到字寬、換行與存檔相容的問題，所以本專案在模擬器裡攔下遊戲自己的畫字程式：

- 遊戲要畫一段英文時，以原文查譯文表，跳過原版的繪字，並回傳原版會回傳的值，遊戲照常往下走。
- 中文用 Noto Sans CJK 畫在 960×600 的覆繪層，圓角描邊；原版畫面放大後墊在底下。
- 對話由片語拼成，插槽在執行期填入物件名稱；覆繪依遊戲解碼片語的順序，逐段組合譯文。
- 選單按鈕、游標動詞、開場字卡是圖片，改以圖塊編號與位置辨識，在原圖上蓋中文。
- 英文原文只當查表鍵。章節片語的鍵存成雜湊，原文不進版控。

| 譯文表 | 內容 | 不重複句數 |
|---|---|---|
| `intro` | 開場字幕 | 47 |
| `chapter1`–`chapter4` | 四章對白、旁白、選單 | 3,506／2,501／298／768 |
| `help-ch1`、`help-ch234` | 說明影片字幕 | 132／172 |
| `credits` | 片頭、片尾名單的職稱與角色名 | 80 |

翻譯以英文為主、法文對照，專名依[譯名表](text/glossary.tsv)，規則見 [text/TRANSLATING.md](text/TRANSLATING.md)。簡中由繁中以 OpenCC `tw2sp` 轉換，再以譯名表的簡中欄覆寫專名。

## 現況與待辦

| 項目 | 狀態 |
|---|---|
| 開場、四章、說明影片、選單、名單 | 有中文，四章與名單在遊戲裡抽樣確認 |
| 繁中、简中、英文原版切換 | 可用 |
| 聲音 | 未輸出（[#3](https://github.com/wicanr2/kgb-dosgolem/issues/3)） |
| 物品插槽句（例：把 X 扔進水裡） | 未實測（[#5](https://github.com/wicanr2/kgb-dosgolem/issues/5)） |
| 公開發行包 | 本機完整版可建，公開版未做（[#1](https://github.com/wicanr2/kgb-dosgolem/issues/1)、[#2](https://github.com/wicanr2/kgb-dosgolem/issues/2)） |

詳細現況在 [CONTEXT.md](CONTEXT.md)，待辦以 [docs/worklist.json](docs/worklist.json) 為準，並對應 [GitHub issues](https://github.com/wicanr2/kgb-dosgolem/issues)。

## 執行

需要合法的 KGB CD 版。本庫不含遊戲檔、影片、音訊與字型；`docs/images/` 只有上面幾張中文化截圖。

建置與執行都在 Docker 內，並需要旁邊的 `../ferrogolem` 與主機的 Noto Sans CJK 字型（`NotoSansCJK-Medium.ttc`）。先依 [docs/re/001](docs/re/001-disc-inventory.md) 從光碟映像解出檔案，再：

```sh
tools/rust.sh fetch                 # 抓依賴，只有這一步開網路
tools/rust.sh build
tools/rust.sh test
tools/rust.sh play                  # 在 X11 桌面開視窗玩；預設語言依系統語系
tools/package.sh with-data all      # 本機完整版（Linux AppImage、Windows、macOS，內含遊戲），放 dist-all/
```

本機完整版內含原版資料，只供自己使用，請勿散布。

## 文件

- [工作契約](AGENTS.md)、[目前狀態](CONTEXT.md)、[歷程](WORKLOG.md)
- [證據索引](docs/re/README.md)
- 規格：[文字接手](docs/spec/004-kgb-text-takeover.md)、[圖片文字](docs/spec/005-sprite-labels.md)、[名單](docs/spec/006-credits.md)

## 授權

自有程式與文件採 RRSAL-1.0（復古重製 source-available 授權：非商業免費使用、允許實況影片），全文見 [LICENSE](LICENSE)。授權不涵蓋原版遊戲的程式、影像、聲音與文字。字型 Noto Sans CJK 採 SIL Open Font License 1.1。

《KGB》著作權屬 Cryo Interactive、Virgin Games 及其權利繼承者。本專案與上述公司沒有隸屬或授權關係。
