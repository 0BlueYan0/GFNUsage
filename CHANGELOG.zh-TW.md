# 更新紀錄

[English](CHANGELOG.md)

格式照 [Keep a Changelog](https://keepachangelog.com/zh-TW/1.1.0/)，版本號照
[語意化版本](https://semver.org/lang/zh-TW/)。

每一個發佈出去的版本都要在這裡有一段。發佈流程會把 tag 對應的那一段讀進
release 說明，沒有就不建置。

## [Unreleased]

## [0.1.3] - 2026-09-22

### 修正

- 面板顯示時真的會重新載入。0.1.2 送了事件，但視窗沒有被授予訂閱事件的權限，
  所以那個事件到不了前端，面板照樣停在啟動時讀到的數字。

## [0.1.2] - 2026-09-22

### 修正

- 面板顯示的是當下的數字。以前只在啟動時讀一次就不再讀，所以系統匣有數字了，
  旁邊的面板還寫著沒有資料。
- 檢查更新最多等 12 秒，走系統 proxy 失敗會直連再試一次。以前一次檢查可以卡
  將近兩分鐘，而且連得到 NVIDIA 的 proxy 不一定連得到 GitHub。
- 檢查更新期間不再把主面板的按鈕鎖住。

## [0.1.1] - 2026-09-22

### 修正

- 檢查更新會講結果：已經是最新版，或是為什麼查不到。以前這兩種情況畫面都不動。
- macOS 上安裝更新會裝完並重新啟動。以前會讓關於頁一直停在「更新安裝中」。
  Windows 不受影響。
- 更新橫幅的文字和旁邊的按鈕對齊了。
- 設定頁捲動時標題和返回按鈕留在畫面上，跟遊玩紀錄那一頁一樣。

## [0.1.0] - 2026-09-21

第一個公開版本。實際上只有 Windows —— macOS 由 CI 建出來，沒有人跑過。

### 新增

- 在 Windows 系統匣與 macOS 選單列顯示這個月還剩幾小時。
- 配速門檻與超支預測，分母是真的能玩的時間，不是牆上時鐘。
- 今天還能玩多久，以及照這個速度月底會剩多少沒用掉。
- 最近玩了什麼：每一場的遊戲、時間、長度。
- 在面板裡設定不可遊玩時段，可以匯出匯入。
- 用 NVIDIA 帳號在 webview 裡登入，之後自己維持登入狀態。
- 查有沒有新版本，按一下就裝。
- 開機時自動啟動。

[Unreleased]: https://github.com/0BlueYan0/GFNUsage/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/0BlueYan0/GFNUsage/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/0BlueYan0/GFNUsage/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/0BlueYan0/GFNUsage/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/0BlueYan0/GFNUsage/releases/tag/v0.1.0
