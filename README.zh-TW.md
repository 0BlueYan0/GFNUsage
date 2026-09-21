# GFNUsage

GeForce NOW 月配額用量監控。常駐在 **Windows 系統匣**與 **macOS 選單列**，
一眼看到剩餘時數，並告訴你以目前速度會不會用完。

[English](README.md)

> **狀態：Windows 上的第一版功能到齊。**
> 用 NVIDIA 帳號在 webview 裡登入，之後自己維持登入狀態，在系統匣顯示
> 剩餘時數、配速門檻、超支預測、逐場遊玩紀錄，以及在面板裡設定的不可
> 遊玩時段。
>
> **macOS 尚未實測。** 登入流程正是為了讓沒裝 GFN 客戶端的 Mac 也能自己
> 登入而做的，但開發機是 Windows，還沒有人在 Mac 上跑過。

## 為什麼做這個

GeForce NOW 的 Performance 與 Ultimate 方案有每月 100 小時上限，
未用完最多結轉 15 小時，因此單期上限是 115 小時。
官方客戶端只在設定頁深處給你一個數字，不會告訴你用得太快還是太慢。

## 功能

- ✅ **常駐顯示剩餘時數** —— 不必開 GFN 客戶端翻設定
- ✅ **結轉與加購時數** —— 併入本期總額一併顯示
- ✅ **配速門檻** —— 算出「到現在為止該用多少」，超前就變紅
- ✅ **超支預測** —— 會在哪天用完、超出多少
- ✅ **不可遊玩時段** —— 設定上班、睡眠等時段，預測改用「可遊玩時間」當分母。
  週末的空閒密度是平日的兩倍，用牆上時鐘算會把週六晚上正常遊玩誤判成超支
- ✅ **「今天還能玩多久」** —— 把剩餘額度攤到本期剩下的可遊玩時間
- ✅ **結轉浪費預警** —— 預估月底會剩多少、其中多少會因超過 15 小時上限而作廢
- ✅ **用 NVIDIA 帳號登入** —— 在 webview 裡走 OAuth，沒裝 GeForce NOW
  客戶端的機器也能自己登入
- ✅ **最近玩了什麼** —— 每一場的遊戲、時間、長度
- ✅ **設定匯出／匯入** —— 一份不可遊玩時段以 JSON 檔在多台機器之間搬移

## 安裝

到 [Releases](https://github.com/0BlueYan0/GFNUsage/releases) 下載最新的
`GFNUsage_<版本>_x64-setup.exe` 執行。

裝在你的使用者資料夾，不需要系統管理員權限。這個專案沒有買程式碼簽章憑證，
所以 Windows 會跳「已保護您的電腦」並顯示不明的發行者，按**其他資訊** →
**仍要執行**。

程式會自己查有沒有新版並告訴你。你按了才會下載安裝。

**macOS 不保證能跑。** `.dmg` 由 CI 建出來，但沒有人跑過。它未簽章也未公證，
Gatekeeper 第一次會擋下來——在 app 上按右鍵選**打開**，或到
**系統設定 → 隱私權與安全性**放行。

解除安裝會留下三樣東西，這是刻意的，重裝之後接得回去：
`%APPDATA%\tw.iosclub.gfnusage\`、
`%LOCALAPPDATA%\tw.iosclub.gfnusage\`，
以及 Windows 認證管理員裡的 `GFNUsage` 那幾筆。

## 自行建置

需要 Node 22、stable 的 Rust 工具鏈，以及 WebView2 執行階段
（Windows 11 已內建）。

```
npm ci
npm run tauri dev      # 直接跑，會真的打 NVIDIA 的 API
npm run tauri build    # 產出安裝檔
```

## 運作方式

讀 NVIDIA 帳號上的剩餘時數，而不是在本機自己數秒數，
因此在多台電腦上遊玩也算得準。

登入一次就好，在 webview 裡完成。存下來的是一顆一小時的 token，放在作業
系統的金鑰儲存區（Windows 認證管理員 / macOS 鑰匙圈），不落純文字檔。
過期時程式在背景自己換新的，只要 webview 的 cookie 還在就不會再問你。

## 免責聲明

本專案使用的是 GeForce NOW 客戶端內部、未公開的介面。
這些介面可能在任何時候變更或失效，屆時本工具會隨之停止運作。

本專案與 NVIDIA Corporation 無任何隸屬關係，未經其背書或贊助。
「NVIDIA」、「GeForce」、「GeForce NOW」為 NVIDIA Corporation 之商標。

## 授權

[Apache License 2.0](LICENSE)
