fn main() {
    // tauri-build 預設嵌的 manifest 只有 Common-Controls，沒有 supportedOS。
    // 沒宣告 Windows 8 以上，WS_EX_LAYERED 就只能用在頂層視窗，工作列 widget
    // 掛進 Shell_TrayWnd 的那扇子視窗會建不起來。
    //
    // app.manifest 裡的三個 GUID 依序是 Windows 8、8.1、10 與 11。說明寫在這裡
    // 而不是 manifest 的 XML 註解：嵌進資源時非 ASCII 字元會壞掉，程序啟動時
    // 報 14001（並列設定不正確）。
    println!("cargo:rerun-if-changed=app.manifest");
    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("tauri-build 失敗");
}
