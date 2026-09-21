/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  // 不出 sourcemap。release 建構關掉 devtools，沒有人看得到它，
  // 而那份 map 有 400KB，會整包進安裝檔。
  build: { target: "es2021", sourcemap: false },
  // globals 必須是 true：Testing Library 靠全域的 afterEach 自動清掉上一個
  // 測試留下的 DOM，關掉的話同一個檔案裡第二個 render 會找到兩份元素。
  test: { environment: "jsdom", globals: true },
});
