# [0.3.0](https://github.com/0BlueYan0/GFNUsage/compare/v0.2.0...v0.3.0) (2026-09-26)


### 錯誤修正

* **app:** macOS 啟動時 Dock 圖示不再出現 ([42f3908](https://github.com/0BlueYan0/GFNUsage/commit/42f3908a49b19c62d831aafc32706083882fda14))
* **app:** macOS 下載後打得開 ([680799a](https://github.com/0BlueYan0/GFNUsage/commit/680799ad9686c559d92e8eef761280853a8d4f88))
* **panel:** Windows 上再點一次系統匣是把面板拉到前面，不是收起 ([17084a2](https://github.com/0BlueYan0/GFNUsage/commit/17084a2a00e0d4dc5f8416be2ed68f39f19d9540))
* **panel:** macOS 面板開在選單列圖示所在的那台螢幕 ([3e26805](https://github.com/0BlueYan0/GFNUsage/commit/3e268058b4f82faedb16809981e964ede5d87c2b))
* **panel:** macOS 第一次啟動的面板也開在選單列圖示下方 ([a58c5bb](https://github.com/0BlueYan0/GFNUsage/commit/a58c5bb9a360942093b1d4e5d82fc3c8f0a86565))
* **panel:** macOS 面板開在選單列圖示下方，再點一次收起 ([4d4882a](https://github.com/0BlueYan0/GFNUsage/commit/4d4882a7e149df18c6524ceab88068e5bac62383))
* **tray:** 沒有資料時，淺色選單列上的項目看得清楚 ([d84578d](https://github.com/0BlueYan0/GFNUsage/commit/d84578dda9c30a2dfb7476a582ae3a17fdf6b0bd))


### 新功能

* **app:** macOS 不佔 Dock，登入時例外 ([96dd1e5](https://github.com/0BlueYan0/GFNUsage/commit/96dd1e5a560f4c4e06fccec342374a7986ef9b8f))
* **tray:** macOS 選單列顯示剩餘時間與進度條 ([4eb3c0d](https://github.com/0BlueYan0/GFNUsage/commit/4eb3c0d7b2d5119825fc52c77de504ea2e07c565))

# [0.2.0](https://github.com/0BlueYan0/GFNUsage/compare/v0.1.3...v0.2.0) (2026-09-23)


### 錯誤修正

* **app:** 資料更新設成關閉時，啟動仍抓一次 ([1ca6066](https://github.com/0BlueYan0/GFNUsage/commit/1ca6066d94c727509bb84560a67d76c0fa0091df))
* **ui:** 更新間隔那一列改叫「資料更新」，不再跟程式更新同名 ([44c22e0](https://github.com/0BlueYan0/GFNUsage/commit/44c22e0a9d57446a3086f6513d56f41e338d017f))
* **ui:** 走勢圖改用最後一次抓到的遊玩紀錄重畫，登出時清掉 ([aa8d879](https://github.com/0BlueYan0/GFNUsage/commit/aa8d879291cad2b562f11e41355ef4c2924b3825))
* **ui:** 主面板不再因為 1px 的溢出冒出捲軸 ([7595dc0](https://github.com/0BlueYan0/GFNUsage/commit/7595dc08c0eb355a5ed71a60eec16b51be178437))
* **ui:** 進度條不再被壓到看不見 ([b32bff4](https://github.com/0BlueYan0/GFNUsage/commit/b32bff465cdd0a8cc99db1dbe18eeb2fd9d8411b))


### 新功能

* **pace:** 預測改用最近七天，不用整期平均 ([4b66715](https://github.com/0BlueYan0/GFNUsage/commit/4b667155e4e623ad5c2eefa11b75402d17fce527))
* **pace:** 預測會超支時也轉紅 ([26493f0](https://github.com/0BlueYan0/GFNUsage/commit/26493f0dd3153bf24e46cba91d3b2a2c7fced218))
* **ui:** 主面板加上本期走勢圖 ([75dd9ef](https://github.com/0BlueYan0/GFNUsage/commit/75dd9efd61266d7235c93007933d3efbe7b5a5d5))
* **ui:** 可以選資料多久更新一次，並接上視窗偵測 ([9e2723d](https://github.com/0BlueYan0/GFNUsage/commit/9e2723de0f7b29e4a47a298523dd0a722a873d10))
* **ui:** 走勢圖標出今天與期末的百分比 ([b0fa976](https://github.com/0BlueYan0/GFNUsage/commit/b0fa976eaeb5d9afaaf54ac8570488fb0c40ab6f))
* **ui:** 配速標記凸出進度條上下 ([0c64f0f](https://github.com/0BlueYan0/GFNUsage/commit/0c64f0f9747339b3a7908bd2bc5d591c0bfadd6f))
* **ui:** 進度條上標出配速門檻 ([2b72b0b](https://github.com/0BlueYan0/GFNUsage/commit/2b72b0b12345e4f4612726802b18a11aed358711))
* **ui:** 預測那幾句移進走勢圖的 tooltip ([8a1e80b](https://github.com/0BlueYan0/GFNUsage/commit/8a1e80b95269ea0c40d419820232571c75b03fc6))
* **ui:** 走勢圖加上百分比刻度 ([7f234ce](https://github.com/0BlueYan0/GFNUsage/commit/7f234ceee0259a7e2545314e5f3bcecf733f7ed0))
* **watcher:** 讀 GeForce NOW 視窗的狀態 ([522c4af](https://github.com/0BlueYan0/GFNUsage/commit/522c4af4af492f2ce060cdb482f7cc749e96f77b))
* **widget:** 在 Windows 工作列上顯示剩餘或已使用的時數 ([d9857a2](https://github.com/0BlueYan0/GFNUsage/commit/d9857a25f8817e5739439f704423a6d96e5732b7))

## [0.1.3](https://github.com/0BlueYan0/GFNUsage/compare/v0.1.2...v0.1.3) (2026-09-21)


### 錯誤修正

* **app:** 面板視窗可以接收事件 ([35093ec](https://github.com/0BlueYan0/GFNUsage/commit/35093ec8fbf509d5cf9cdabd739918bf9d1f32b1))

## [0.1.2](https://github.com/0BlueYan0/GFNUsage/compare/v0.1.1...v0.1.2) (2026-09-21)


### 錯誤修正

* **app:** 檢查更新加上逾時，失敗改直連 ([10287e2](https://github.com/0BlueYan0/GFNUsage/commit/10287e2acbd6529d9b6b97fd02efdd02c92525d5))
* **ui:** 面板顯示時重新載入 ([aab0f15](https://github.com/0BlueYan0/GFNUsage/commit/aab0f15f6feafb258a40504054e05bfe9c091a93))
* **ui:** 關於頁不再鎖住主面板 ([02cd379](https://github.com/0BlueYan0/GFNUsage/commit/02cd3790094eb16ad5f0cef605aa8f1530d19cfb))

## [0.1.1](https://github.com/0BlueYan0/GFNUsage/compare/v0.1.0...v0.1.1) (2026-09-21)


### 錯誤修正

* **app:** 檢查更新會講結果，macOS 安裝會裝完 ([9f6db8d](https://github.com/0BlueYan0/GFNUsage/commit/9f6db8d418685f043128efe6ae284258ed3a8c48))
* **ui:** 橫幅文字與按鈕對齊 ([7682fcd](https://github.com/0BlueYan0/GFNUsage/commit/7682fcdebc6acde3116bcf6f45bd4d7663fa38cf))
* **ui:** 設定頁標題釘住，跟遊玩紀錄頁一樣 ([dea5e53](https://github.com/0BlueYan0/GFNUsage/commit/dea5e53bb04905af55e6a6fa0c53a035fffe6fa0))

# [0.1.0](https://github.com/0BlueYan0/GFNUsage/compare/a9a0e42b6e9d52c66f1216ba8b4647ac6864c596...v0.1.0) (2026-09-21)


### 錯誤修正

* **app:** 更新安裝失敗會顯示出來，並補上關閉提示的測試 ([cf50088](https://github.com/0BlueYan0/GFNUsage/commit/cf5008840a356a5f6a203febf7eb6bd318f0626a))
* **auth:** 登入流程可診斷、可中斷，逾時放寬 ([86544fa](https://github.com/0BlueYan0/GFNUsage/commit/86544faf7830d13259083c61eef80e2930c54029))
* **auth:** 登入要打兩次，不是一次 ([1f23c05](https://github.com/0BlueYan0/GFNUsage/commit/1f23c059a84a9d140131e13dc8c742528ba78533))
* 憑證換掉時丟棄快取的 token ([b9157ca](https://github.com/0BlueYan0/GFNUsage/commit/b9157ca9d7b26e5a045706b2bdf27365cba73935))
* 依 review 補強憑證儲存與刷新流程 ([155c906](https://github.com/0BlueYan0/GFNUsage/commit/155c9060226194f431f48686b7bc35018c9e4ef0))
* 設定檔錯誤不再蓋掉憑證錯誤 ([33bae7e](https://github.com/0BlueYan0/GFNUsage/commit/33bae7edce526c5bfeab2f53926a2f9fc6861af1))
* 點第一下系統匣就開面板 ([bebf454](https://github.com/0BlueYan0/GFNUsage/commit/bebf454c65ddb177c547541e9e6afb04dfddeade))
* 快照一變就重畫系統匣 ([ff7d36d](https://github.com/0BlueYan0/GFNUsage/commit/ff7d36deb5d7ce7c32e65f57ad7234f8bf9682e8))
* 不再每次啟動都鑄一顆新 token ([c46c1e6](https://github.com/0BlueYan0/GFNUsage/commit/c46c1e628d733524732e0243b96c69f3ce8b3dde))
* 依 review 顯示被吞掉的錯誤，並修設定表單 ([3bb1676](https://github.com/0BlueYan0/GFNUsage/commit/3bb1676ba0b498860e5c06629815833ccefd49f5))
* **ui:** Alt+F4 收起面板，不結束程式 ([3ce117f](https://github.com/0BlueYan0/GFNUsage/commit/3ce117fc84be19c5ab07000f8f4398073b0d37f4))
* **ui:** 面板內容與標題同寬 ([d300dca](https://github.com/0BlueYan0/GFNUsage/commit/d300dca9dca39e280b75229e9089f59209dc23db))
* **ui:** 日期與時間之間固定用 ASCII 空格 ([da78b88](https://github.com/0BlueYan0/GFNUsage/commit/da78b88dd45d774777213114da65e1ed6010fc99))


### 新功能

* 面板貼在系統匣那一角 ([6a8940d](https://github.com/0BlueYan0/GFNUsage/commit/6a8940dc53f66a6ed64a3faada37ea49ca91b32e))
* **api:** 抓取並解析 NVIDIA 訂閱資料 ([60638f9](https://github.com/0BlueYan0/GFNUsage/commit/60638f989322839f0cda0bf5cc6142f818afa80b))
* **api:** 讀取逐場遊玩紀錄 ([4b0c633](https://github.com/0BlueYan0/GFNUsage/commit/4b0c63364c8ff6c2e4136e890621cfb088a402ff))
* **api:** 今日用量改從逐場紀錄算 ([dac333b](https://github.com/0BlueYan0/GFNUsage/commit/dac333b2b5a77f3f258f8a6b2ff1ddd16ca76315))
* **app:** 檢查更新並提供安裝 ([4005c26](https://github.com/0BlueYan0/GFNUsage/commit/4005c26a451e000ab4755e8a0dc468cddaf20929))
* **app:** 只跑一個實例，診斷訊息寫進檔案 ([ed9b63a](https://github.com/0BlueYan0/GFNUsage/commit/ed9b63a1d43b914d641b0545cb0fbeffbfe0cdbb))
* **auth:** 加入走 localhost 回呼的 OAuth 登入 ([c5ab0ae](https://github.com/0BlueYan0/GFNUsage/commit/c5ab0ae4353d2407b148557ad7eb1ed0e91a75f5))
* **auth:** 加入 TokenManager，刷新一次只跑一份並輪替 ([a27a7f2](https://github.com/0BlueYan0/GFNUsage/commit/a27a7f22754e65de730457f2036f4efb6db81802))
* **auth:** 加入 TokenStore trait，有金鑰儲存區與記憶體兩種後端 ([266013e](https://github.com/0BlueYan0/GFNUsage/commit/266013e95f831b7db955b2dde3099a5e139cba05))
* **auth:** 組出 PKCE challenge 與授權網址 ([c46d7a9](https://github.com/0BlueYan0/GFNUsage/commit/c46d7a97ae978d753f03c711aeb90fb1a8b0eef1))
* **auth:** 解讀 GFN 客戶端的 session 資料 ([a9a0e42](https://github.com/0BlueYan0/GFNUsage/commit/a9a0e42b6e9d52c66f1216ba8b4647ac6864c596))
* **auth:** 用授權碼換憑證 ([dcc62bb](https://github.com/0BlueYan0/GFNUsage/commit/dcc62bbfba657c29497ba196815afcdadfb7f355))
* **auth:** 讀 JWT 的到期欄位 ([52b48ee](https://github.com/0BlueYan0/GFNUsage/commit/52b48ee70f8430f448fc173d9a46ff41dd497600))
* **auth:** 在 loopback 埠接收授權碼 ([893cbbb](https://github.com/0BlueYan0/GFNUsage/commit/893cbbb39ae2e4a5d0f686bb0291c7891e290983))
* **auth:** 記住 client token 何時到期 ([cf7d36d](https://github.com/0BlueYan0/GFNUsage/commit/cf7d36db69b1de60106edf43adaf5534ca021024))
* **auth:** 改在 webview 裡登入，不走 loopback 埠 ([883e226](https://github.com/0BlueYan0/GFNUsage/commit/883e226c7f9ac90e04556c1ade117610958a50d7))
* 匯出與匯入不可遊玩時段 ([96fe50a](https://github.com/0BlueYan0/GFNUsage/commit/96fe50add9eed46478749557d169a5932d3b5601))
* **pace:** 加入不可遊玩時段的模型與設定檔 ([9e4cba0](https://github.com/0BlueYan0/GFNUsage/commit/9e4cba06420a1bdbda7a4ca4efc564dfdedb0290))
* **pace:** 從不可遊玩時段算出可遊玩分鐘數 ([8b2669f](https://github.com/0BlueYan0/GFNUsage/commit/8b2669f65f3213547b1e27f747ced6c31477e206))
* **pace:** 算出配速門檻、預測與今日額度 ([7d8f3ce](https://github.com/0BlueYan0/GFNUsage/commit/7d8f3ce67b4cfa8aa9fd6bd29e739799cccec4fb))
* **pace:** 沿著可遊玩區間找出配額用完的時刻 ([52ebded](https://github.com/0BlueYan0/GFNUsage/commit/52ebded113f2e9a3d0c65a76b7281e034942bd09))
* 機器一醒來就再抓一次 ([6ecf1ed](https://github.com/0BlueYan0/GFNUsage/commit/6ecf1ed35f60430e1855b38675e6efa7fecd866b))
* **quota:** 從訂閱資料算出顯示用的快照 ([74a53b1](https://github.com/0BlueYan0/GFNUsage/commit/74a53b185dfa22ec71e06d89f03c2fb46d3acde6))
* 每次輪詢重算配速，並開出時段設定的指令 ([615e707](https://github.com/0BlueYan0/GFNUsage/commit/615e7076f3c015b0adf8ef2c2866a3a3d38b5749))
* **store:** 累積快照歷史 ([c08f7d4](https://github.com/0BlueYan0/GFNUsage/commit/c08f7d46ebdb5bce0d7ee0c4699358c1172192fd))
* **tray:** 把剩餘時數畫成系統匣圖示 ([35a61a3](https://github.com/0BlueYan0/GFNUsage/commit/35a61a3d937bff3c69c8ae42b2fba03396333d29))
* **tray:** 顯示超前消耗狀態，配額用完時標示 ([ec0eb1e](https://github.com/0BlueYan0/GFNUsage/commit/ec0eb1e7084c7b342e2e18576e2287162f94a700))
* **ui:** 加入面板，可匯入憑證與顯示配額 ([ec60319](https://github.com/0BlueYan0/GFNUsage/commit/ec603199cc3fca5c0be47c7b35205e1b5fea235e))
* **ui:** 加入不可遊玩時段編輯器 ([59ac834](https://github.com/0BlueYan0/GFNUsage/commit/59ac834949843463efe18c7575cbf8c3781babda))
* **ui:** 倒數到小時，不只到天 ([641e894](https://github.com/0BlueYan0/GFNUsage/commit/641e894c0975dd6cb3a3696f8bbf1d36e73d9aa8))
* **ui:** 遊玩紀錄獨立一頁 ([87ec1fd](https://github.com/0BlueYan0/GFNUsage/commit/87ec1fd3727dc89021d05000a9eaffa3eb035fb1))
* **ui:** 面板顯示配速門檻與預測 ([56d6d4e](https://github.com/0BlueYan0/GFNUsage/commit/56d6d4e65271ab44c390618187961cc7c5f064b6))
* **ui:** 關於頁顯示版本、更新與開機啟動 ([9deddee](https://github.com/0BlueYan0/GFNUsage/commit/9deddeeadd72688013491b5482d2a7d5a10734f1))
* **ui:** 面板只留數字，並修正今日額度 ([6330010](https://github.com/0BlueYan0/GFNUsage/commit/63300101f24739e496f601769a35694348b1fb2c))
* **ui:** 憑證到期前提醒，並提示系統匣的收合區 ([4ad0207](https://github.com/0BlueYan0/GFNUsage/commit/4ad020702e612e9496c53343b39aa145074086b3))
* 系統匣圖示接上訂閱輪詢迴圈 ([3f5464c](https://github.com/0BlueYan0/GFNUsage/commit/3f5464ce24cddf69e13e77ef041830b9786b166d))
