# Mobile onboarding 與 Widgets 設計

更新：2026-09-26。本文件描述 iOS onboarding、WidgetKit 與 ActivityKit 的目前設計。連線與部署另見 [Cloudflare 測試](../guides/cloudflare-testing.md)。

## 參考方向

透過 iPhone Mirroring 觀察已安裝的兩個 app，沿用它們已建立的資訊層級：

- **Nowdex 的狀態首頁**：小型 provider 圖示、緊湊的用量卡片、細刻度進度條、標籤靠左／數值靠右、次要重置時間。
- **Nomo 的 Home 設定頁**：簡短介紹、依實際狀態完成的設定清單、加入 widget 與編輯 widget 的分段教學。

這次觀察到的是上述頁面，未完整操作兩款 app 的內部 widget 選擇頁。Codync 以自己的 SwiftUI 元件、角色圖示與教學重建這些模式，沒有匯入兩款 app 的程式碼、影片或素材。

## Onboarding 與帳號

1. 首次開啟由 Welcome 提供 Get started 與 Google 登入，再進入配對／帳號電腦流程。
2. 儲存第一台配對電腦後，記錄此裝置的 `onboardingCompleted`。
3. 既有已配對安裝自動視為完成 onboarding。
4. 完成後才顯示帳號入口；切換到沒有電腦的帳號，或解除配對後，入口仍保留，避免無法切回原帳號。
5. 「帳號」代表 Codync 的 Google／Clerk 登入身分。帳號與 computer 是不同概念。

目前完成條件沿用現有的配對資料儲存事件，不代表已驗證雲端登入或 host 當下在線。Welcome、配對與帳號電腦的選擇由 RootView 和帳號狀態共同決定。

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
- App 與 widget 的非同步用量回應都核對原始帳號 context 與 computer ID，避免切換途中收到的舊資料覆蓋新電腦。
- Bot 新增、改名、活動變化、隱藏、刪除與清空都會更新 widget feed；刷新判斷包含 widget 實際顯示的欄位。
- Bot 連結帶有帳號與電腦範圍，舊 widget 的連結不會直接開啟其他帳號的 bot。
- Provider widget 點擊開啟 Usage。Bot 的中尺寸可點擊個別 bot。
- Widgets 顯示最近回報的狀態；即時聊天與審批在 App 完成。WidgetKit 排程由系統決定，不保證即時更新。
- Bots 由 App 更新快取後重載；用量透過 HostConnector 連線（direct 或 relay），失敗時使用快取。

## 驗證

執行 `swift test --package-path kit` 與 `python3 tools/render-widgets.py` 檢查共用邏輯及繪製。系統 WidgetKit／ActivityKit 預覽在 `apps/ios/Widgets/WidgetPreviews.swift`。

實機仍需分別確認加入 Home／Lock Screen widget、編輯 provider、跨帳號切換、背景刷新，以及 Live Activity／APNs 更新。共用圖片與 build 成功不等於這些系統整合已通過。[歷史檢查紀錄](../archive/mobile-widgets-verification-2026-09-26.md) 保留當時的測試範圍與操作限制。

## Halftone 與 Thinking Orbs（2026-09-26）

Bot 的角色頭像使用 `CharacterAvatar` 點陣；provider 圖示由 `ProviderMascot` 選擇，小尺寸可使用隨 app 打包的官方 provider 圖示。15 pt 用 7 × 7、20–26 pt 用 9 × 9、28 pt 以上用 13 × 13 網格，保留鏤空眼睛並提高小尺寸墨色。iOS 與 macOS 共用 provider 呈現邏輯。

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
