# Mobile onboarding 與 Widgets 設計

更新：2026-09-25。此工作範圍是 iOS 介面與 WidgetKit；Cloudflare 後端由 Claude Code 另行實作。

## 參考方向

透過 iPhone Mirroring 觀察已安裝的兩個 app，沿用它們已建立的資訊層級：

- **Nowdex 的狀態首頁**：小型 provider 圖示、緊湊的用量卡片、細刻度進度條、標籤靠左／數值靠右、次要重置時間。
- **Nomo 的 Home 設定頁**：簡短介紹、依實際狀態完成的設定清單、加入 widget 與編輯 widget 的分段教學。

這次觀察到的是上述頁面，未完整操作兩款 app 的內部 widget 選擇頁。Codync 以自己的 SwiftUI 元件、角色圖示與教學重建這些模式，沒有匯入兩款 app 的程式碼、影片或素材。

## Onboarding 與帳號

1. 首次開啟先顯示連接電腦流程，隱藏左上角 profile／帳號切換入口，也不彈出帳號選單。
2. 儲存第一台配對電腦後，記錄此裝置的 `onboardingCompleted`。
3. 既有已配對安裝自動視為完成 onboarding。
4. 完成後才顯示帳號入口；切換到沒有電腦的帳號，或解除配對後，入口仍保留，避免無法切回原帳號。
5. 「帳號」代表 Codync 的 Google／Clerk 登入身分。帳號與 computer 是不同概念。

目前完成條件沿用現有的配對資料儲存事件，不代表已驗證雲端登入或 host 當下在線。待雲端 onboarding 整合時，應由單一流程明確管理登入、電腦註冊與完成狀態。

## Widget 與 App 內預覽

| 類型 | Small | Medium |
| --- | --- | --- |
| Provider usage | provider、最接近上限的用量、細刻度條、重置時間 | 同一 provider 的兩個用量視窗與重置時間 |
| Bots | 待回應／工作中的數量與角色圖示 | 狀態摘要與最多三個 bot，待回應優先 |

- Provider usage 可在系統「編輯 Widget」選擇 Claude 或 Codex。
- Claude 使用既有暖色；Codex 使用藍色；90% 以上顯示警示色。
- 採平面深淺色表面、小型圖示、11–13 pt 標籤；大字只用於主要數值。
- App 的 Usage 卡片同步縮小字級、圖示、內距，並使用同一款刻度條。
- Widgets 頁可從 Settings 或 Usage 的「Widgets & setup」進入，提供類型／provider 切換、小／中預覽與設定教學。
- 設定清單讀取實際電腦配對狀態與 `WidgetCenter` 的已安裝配置；不以點過教學當作完成。
- 查詢失敗時顯示錯誤與「Check again」，不會永久停在載入狀態。
- 預覽明確標示 Sample data。已安裝的 widget 不用範例資料掩蓋未配對或尚無用量的狀態。
- App 教學、設定清單與說明支援 Dynamic Type；無障礙大字體改用選單切換類型／provider。固定大小的 widget 保留緊湊數值，並提供完整的 VoiceOver 用量與重置描述。
- 教學涵蓋 Home Screen 與 Lock Screen；新增 WidgetKit 原生預覽涵蓋小／中尺寸、矩形／圓形／行內配件與空狀態。

`kit/Sources/CodyncKit/Design/WidgetCards.swift` 是共用繪製元件；`apps/ios/Views/WidgetGalleryView.swift` 與 `apps/ios/Widgets/CodyncWidgets.swift` 共用它，減少預覽與實際 widget 的差異。

## 資料與互動

- Widgets 跟隨 App 目前選取的帳號，讀取該帳號的 App Group 快取；不合併不同帳號的 bots 或用量。
- 切換帳號時由既有 App 流程重載 widgets；非同步用量請求保留原 storage context，避免舊回應寫入新帳號。
- 切換或移除目前電腦時清除舊用量、bot 快取與偏好 URL；更新同一台電腦的地址則保留快取。
- App 與 widget 的非同步用量回應都核對 computer token，避免切換途中收到的舊資料覆蓋新電腦。
- Bot 新增、改名、活動變化、隱藏、刪除與清空都會更新 widget feed；刷新判斷包含 widget 實際顯示的欄位。
- Bot 連結帶有帳號與電腦範圍，舊 widget 的連結不會直接開啟其他帳號的 bot。
- Provider widget 點擊開啟 Usage。Bot 的中尺寸可點擊個別 bot。
- Widgets 顯示最近回報的狀態；即時聊天與審批在 App 完成。WidgetKit 排程由系統決定，不保證即時更新。
- Bots 由 App 更新快取後重載；用量沿用目前直接連 host 的刷新與快取退路，未加入新的 Cloudflare API。

## 驗證與後續

已完成：

- iOS Simulator build，含 widget extension 與原生 WidgetKit 預覽巨集，沒有編譯警告。
- Swift package 10 個測試，包含跨帳號隔離與跨電腦 widget 快取清除／保留的回歸測試。
- 首次 onboarding 隱藏帳號入口；完成後，即使沒有電腦仍保留帳號入口。
- App 內 Widgets 頁、Claude／Codex 預覽切換、淺色／深色與無障礙大字體佈局檢查。
- 共用卡片的離線圖片輸出：Claude、Codex、接近上限、Bots 工作／待回應、空清單；每種包含小／中尺寸及淺色／深色。
- 本機 HTTP／SSE 測試 host 到 App Group 的整合檢查：初次 bot／用量同步、狀態不變時改名及活動更新、刪除 bot、切換到離線電腦後清除上一台資料。
- 系統 Add Widget 搜尋能找到 Codync。測試配對不會保留在模擬器。

驗證限制：系統 widget 搜尋結果沒有提供可點擊的 accessibility 元素，座標點擊與捲動仍回報 `noWindowsAvailable`，鍵盤導覽也未能開啟結果。因此尚未在系統 Home／Lock Screen 上實際加入 widget、編輯 provider 或量測背景刷新排程；原生預覽可編譯與共用卡片圖片檢查不等同這些整合驗證。

重現方式：

```sh
swift test --package-path kit
python3 tools/render-widgets.py
```

圖片輸出在 `build/widget-previews/widgets-light.png` 與 `widgets-dark.png`。腳本使用 macOS 的 SwiftUI ImageRenderer，直接繪製產品共用元件；產物不含使用者資料。實際 WidgetKit 預覽在 `apps/ios/Widgets/WidgetPreviews.swift`，可於 Xcode canvas 切換 timeline 的 provider／空狀態。

帳號／雲端架構與 SSH computer 路線分別見 [帳號與裝置計畫](cloudflare-account-device-plan.md) 與 [Bot／computer／SSH 計畫](bot-computer-ssh-plan.md)。
