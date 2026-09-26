# Widget verification record — 2026-09-26

> Historical observations from the original UI work, not a current test result. See [current widget documentation](../design/mobile-widgets.md).

## 驗證與後續

已完成：

- iOS Simulator build，含 widget extension 與原生 WidgetKit 預覽巨集，沒有編譯警告。
- Swift package 14 個測試，包含跨帳號隔離、跨電腦 widget 快取清除／保留，以及 Live Activity 錯誤／未知／stale／計時狀態的回歸測試。
- 首次 onboarding 隱藏帳號入口；完成後，即使沒有電腦仍保留帳號入口。
- App 內 Widgets 頁、Claude／Codex 預覽切換、淺色／深色與無障礙大字體佈局檢查。
- 共用卡片的離線圖片輸出：Claude、Codex、接近上限、Bots 工作／待回應、空清單；每種包含小／中尺寸及淺色／深色。
- 本機 HTTP／SSE 測試 host 到 App Group 的整合檢查：初次 bot／用量同步、狀態不變時改名及活動更新、刪除 bot、切換到離線電腦後清除上一台資料。
- 系統 Add Widget 搜尋能找到 Codync。測試配對不會保留在模擬器。
- 大型 widget 用四個限制視窗及六個 bot 測試實際共用元件的排版；三種鎖定畫面配件也產生了深淺色檢查圖。
- Live Activity 五種狀態 × Lock Screen／compact／minimal／expanded 共用內容已輸出檢查；四種原生 ActivityKit 預覽均可編譯。
- 模擬器實際啟動不連 relay／APNs 的本機範例 Activity，確認系統 Dynamic Island compact 與 Lock Screen 卡片的 Needs you 呈現。Lock Screen 同時出現 iOS 的首次 Allow 提示，未操作該權限提示。

驗證限制：系統 widget 搜尋結果沒有提供可點擊的 accessibility 元素，座標點擊與捲動仍回報 `noWindowsAvailable`，鍵盤導覽也未能開啟結果。因此尚未在系統 Home／Lock Screen 上實際加入 widget、編輯 provider 或量測背景刷新排程；原生預覽可編譯與共用卡片圖片檢查不等同這些整合驗證。

Live Activity 的 compact／Lock Screen 已觀察到系統實際呈現；minimal／expanded 的系統切換及 APNs 背景更新未完成端到端操作驗證。兩個同時活動的範例在此次模擬器中仍顯示 compact，不能據此宣稱 minimal 已驗證。

重現方式：

```sh
swift test --package-path kit
python3 tools/render-widgets.py
```

圖片輸出在 `build/widget-previews/`：`widgets-{light,dark}.png`、`large-widgets-{light,dark}.png`、`activities-{light,dark}.png`、`halftone-icons-{light,dark}.png`。腳本使用 macOS 的 SwiftUI ImageRenderer，直接繪製產品共用元件；產物不含使用者資料。實際 WidgetKit／ActivityKit 預覽在 `apps/ios/Widgets/WidgetPreviews.swift`，可於 Xcode canvas 切換 timeline／content states。

DEBUG simulator build 也可用 `SIMCTL_CHILD_CODYNC_ACTIVITY_PREVIEW=needsInput xcrun simctl launch --terminate-running-process booted com.pokai.Codync.ios` 啟動系統範例。值可為 `working`、`needsInput`、`idle`、`error`、`stale` 或 `multiple`；以 `stop` 結束範例。此入口只存在於 DEBUG simulator，不連 host／relay／APNs，清理僅針對 `codync-design-preview-` 的活動。

帳號／雲端架構與 SSH computer 路線分別見 [帳號與裝置計畫](../archive/cloudflare-account-device-plan-2026-09-25.md) 與 [Bot／computer／SSH 計畫](../archive/bot-computer-ssh-plan-2026-09-25.md)。
