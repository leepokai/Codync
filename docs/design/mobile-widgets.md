# Mobile onboarding 與 Widgets 設計

更新：2026-09-26。此工作範圍是 iOS 介面、WidgetKit 與 ActivityKit；Cloudflare 後端由 Claude Code 另行實作。

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

| 類型 | Small | Medium | Large |
| --- | --- | --- | --- |
| Provider usage | provider、最接近上限的用量、細刻度條、重置時間 | 同一 provider 的兩個用量視窗與重置時間 | 主要用量摘要、最多四個用量視窗 |
| Bots | 待回應／工作中的數量與角色圖示 | 狀態摘要與最多三個 bot，待回應優先 | 數量摘要與最多六個 bot |

既有 Usage limits 保留 Small／Medium 的跨 provider 用量摘要。Bots 與 Usage limits 另支援 Lock Screen 的圓形、矩形與行內形式；行內放在時鐘上方日期列。

- Provider usage 可在系統「編輯 Widget」選擇 Claude 或 Codex。
- Claude 使用既有暖色；Codex 使用藍色；90% 以上顯示警示色。
- 採平面深淺色表面、小型圖示、11–13 pt 標籤；大字只用於主要數值。
- App 的 Usage 卡片同步縮小字級、圖示、內距，並使用同一款刻度條。
- Widgets 頁可從 Settings 或 Usage 的「Widgets & setup」進入，提供類型／provider／大小切換、三種主畫面尺寸預覽與設定教學。
- 「Lock Screen widgets」頁可切換 Bots／Usage limits，預覽圓形、矩形與行內形式。正式 widget 與預覽共用 `AccessoryWidgetCard`。
- 設定清單讀取實際電腦配對狀態與 `WidgetCenter` 的已安裝配置；不以點過教學當作完成。
- 查詢失敗時顯示錯誤與「Check again」，不會永久停在載入狀態。
- 預覽明確標示 Sample data。已安裝的 widget 不用範例資料掩蓋未配對或尚無用量的狀態。
- App 教學、設定清單與說明支援 Dynamic Type；無障礙大字體改用選單切換類型／provider。固定大小的 widget 保留緊湊數值，並提供完整的 VoiceOver 用量與重置描述。
- 教學涵蓋 Home Screen 與 Lock Screen；新增 WidgetKit 原生預覽涵蓋小／中尺寸、矩形／圓形／行內配件與空狀態。

`kit/Sources/CodyncKit/Design/WidgetCards.swift` 是共用繪製元件；`apps/ios/Views/WidgetGalleryView.swift` 與 `apps/ios/Widgets/CodyncWidgets.swift` 共用它，減少預覽與實際 widget 的差異。

## Live Activity 與 Dynamic Island

Settings 與 Widgets 頁都能進入「Live Activity & Dynamic Island」。這裡提供 Live Activities 開關、系統授權狀態，以及形式／任務狀態預覽。預覽明確標示 Sample task，不會啟動實際任務或 Live Activity。

| 形式 | 資訊與互動 |
| --- | --- |
| Lock Screen／橫幅 | bot 名稱、狀態、目前工作、執行時間；點擊回到對話 |
| Compact | 左側 bot、右側計時；需要回應／完成／錯誤時改為狀態圖示 |
| Minimal | 在更小空間顯示狀態圖示，VoiceOver 包含 bot 名稱 |
| Expanded | bot、狀態、工作摘要、回到對話的連結；需要回應時顯示 Respond in Codync |

實際形式由 iOS 根據裝置、活動數量與互動決定；App 內的形式選單只控制設計預覽。[Apple Live Activities 設計指南](https://developer.apple.com/design/human-interface-guidelines/live-activities)

所有形式共用 `BotActivityPresentation` 的狀態判斷：

- Working：顯示目前工作，有開始時間才顯示計時。
- Needs you：顯示待回應提示；審批與文字回應在 App 處理。
- Done：完成勾號與查看結果提示。
- Stopped：錯誤圖示，不會誤顯示成完成。
- Update delayed：ActivityKit 回報 stale 時顯示延遲提示，不繼續顯示即時計時。
- 未知狀態：保守顯示 Waiting for update，不宣稱任務完成。

既有生命週期繼續由手機送出任務時啟動、App 接收事件時更新、完成時結束。App 本地更新設定 15 分鐘 stale date；沒有更新不代表工作失敗。遠端推播沿用現有 relay／host 合約，這輪未變更後端。關閉 Live Activities 會結束目前活動並阻止之後自動啟動。

Dynamic Island 固定黑底，文字採淺色；Lock Screen 卡片配合系統外觀。`ActivityCards.swift` 共用主要內容，正式 Dynamic Island 由系統 region 排版，App 裡顯示示意容器。[Apple DynamicIsland API](https://developer.apple.com/documentation/widgetkit/dynamicisland)

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


## Halftone 與 Thinking Orbs（2026-09-26）

Bot 與 provider 的角色頭像統一使用 `CharacterAvatar` 點陣；小於 24 pt 也不再改用實心剪影。15 pt 用 7 × 7、20–26 pt 用 9 × 9、28 pt 以上用 13 × 13 網格，保留鏤空眼睛並提高小尺寸墨色。iOS 與 macOS 用量卡的 provider 頭像採相同元件。

狀態圖示以 [Jakub Antalik 的 thinking-orbs](https://github.com/Jakubantalik/thinking-orbs) 為來源，這次原生移植四種實際使用的造型：

| 造型 | 使用位置 |
| --- | --- |
| Working／orbits | Bot 工作列、聊天工作提示、工具執行、一般安裝進度、Live Activity 工作狀態 |
| Searching／globe | 搜尋工具執行中 |
| Listening／wave | Bot 待回應列、聊天待回應、Live Activity／Dynamic Island Needs you |
| Connecting／web | 電腦連線提示、帳號登入、agent 登入狀態查詢、fetch 工具、Activity 等待更新 |

角色頭像識別「哪一個 bot」；orb 表達「現在做什麼」。完成、錯誤、更新延遲繼續保留明確的勾號／錯誤／時鐘符號，導航及操作按鈕保留語意圖示。所有 orb 均附於可讀狀態文字或具備外層 accessibility label。

實作採上游 0.3.1、commit `de85557ca220332586d070d8788c0e1d6e877a0d` 的幾何與 20／64 pt 預設值。小於 40 pt 使用精簡密度；其餘使用大尺寸密度。繪製端改用可套色的透明墨色，適配卡片、深淺色及 Dynamic Island。SwiftUI Canvas 不需要 WebView 或 JavaScript runtime。

App 內動畫最多 30 fps；離開畫面、App 非 active 或啟用 Reduce Motion 時顯示靜態幀。Widget、Live Activity 與 Dynamic Island 明確傳入 `animated: false`，不依賴持續動畫計時。狀態仍隨既有資料更新。

上游 MIT 授權全文隨 `CodyncKit` resource bundle 發佈，位於 `kit/Sources/CodyncKit/Resources/ThirdPartyNotices/thinking-orbs-LICENSE.txt`。`ThinkingOrbGeometryTests` 使用上游獨立 golden vectors 的取樣，驗證四種造型 × 兩尺寸 × 四時間點的 dot／line 數量、位置、半徑、墨色及深度順序。其餘五種上游造型暫未移植；有對應產品狀態時再加入。

本次驗證：14 個 Swift package 測試全數通過（含 32 組上游幾何參考案例）；iOS Simulator App／Widget extension build 成功、無編譯警告；確認兩個產物都包含第三方授權。已檢查深淺色 icon 圖版及 Live Activity 共用元件圖版。此驗證不擴大前述原生系統互動／APNs 的完成範圍。
