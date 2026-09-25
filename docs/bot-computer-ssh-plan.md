# Bot、computer、帳號切換與 SSH 遠端執行計畫

日期：2026-09-25  
狀態：規劃；補充 [Cloudflare 帳號與裝置管理計畫](cloudflare-account-device-plan.md)。

## 1. 產品目標

使用者可以在同一個 Codync 介面與多個 bots 聊天，每個 bot 固定使用一台 computer。這台 computer 可以是正在操作的 Mac，也可以是透過網路／SSH 連上的遠端電腦。

例如：

| Bot | Computer | 實際執行位置 | Mac 的連線方式 |
|---|---|---|---|
| Personal reviewer | My MacBook | 本機專案與 agent | 本機連線 |
| Backend maintainer | Dev Linux | 遠端 repo 與 agent | SSH tunnel |
| Build assistant | Mac mini | Mac mini 的工具與工作目錄 | 直連或後續雲端中繼 |

每個 bot 只有一個 computer。預設允許多個 bots 共用同一台 computer；這是多對一歸屬，不是要求每個 bot 獨占一台實體機器。

切換顯示帳號不會移動 bot 的工作位置，也不會把原有 session 搬到另一台電腦。

## 2. 明確區分三種帳號

| 名稱 | 範例 | 管理什麼 |
|---|---|---|
| Codync account | 個人／工作用 Google 或其他 Clerk 登入 | 電腦歸屬、裝置授權、資料隔離 |
| Connection identity | `alice@dev-server`、`deploy@build-machine` | 使用哪個 OS 使用者連到遠端執行環境 |
| Agent identity | Claude Code／Codex 自己的訂閱登入 | 該 agent 的憑證、用量與供應商權限 |

已確認：手機左上角切換不同 Google／Clerk 的 **Codync account**，每個帳號各自管理電腦與 bots。SSH connection identity 僅在 computer 連線設定中選擇，agent identity 留在該 computer 的 agent 登入流程。

同一台實體機器上的兩個 OS 使用者可能各自執行 host、各自有資料庫和權限。computer 應標識一個已驗證的 host 執行環境，而不是僅用 IP、主機名稱或硬體名稱作唯一鍵；不同 OS 使用者的環境不得任意合併。

## 3. 手機與桌面的資訊架構

### 手機

- 左上角：目前帳號 avatar／名稱與展開入口，顯示目前帳號、新增帳號、切換與登出。
- 主清單：目前帳號可存取的 bots；每列顯示 computer 名稱與該 computer 的連線狀態。
- 新增 bot：先選 computer，再取得該 computer 的 agents、模型、專案目錄、connectors 與 skills。
- Bot 設定：顯示固定的 computer；有既存聊天後，換 computer 視為建立新 bot／明確遷移，不直接更改 ID 讓原聊天指向另一台機器。
- Computer 管理：移入帳號內的「Computers」頁，負責新增、連線方式、授權與移除。
- 遠端畫面：放在 bot 或 computer 的操作中；跨 computer 清單沒有唯一的全域 Screen 按鈕。
- 離線：單台電腦離線只影響其 bots，不能把全帳號清單顯示成全部離線。
- 未登入：顯示「Local」或「本機配對」，不能將 computer 名稱誤作已登入的帳號名稱。

### 桌面

- 同一帳號下可以列出本機與遠端 bots，不把 `HostController.store` 視為所有 bot 唯一來源。
- 本機 host 安裝／重啟仍由 `HostController` 管理；遠端連線由獨立 computer connection controller 管理。
- 選到 SSH computer 時，agent 安裝、folder picker、用量、terminal 與設定都必須對應遠端。
- 在操作前清楚顯示 remote computer 和 OS user；不因 SSH 掉線自動把工作轉到本機。

## 4. 資料模型與路由

### 模型

- `AccountContext`：目前可用的 Codync 登入 session、active account ID、該帳號的電腦目錄。
- `Computer`：穩定 ID、owner、驗證過的 host identity、公用顯示資訊與能力。
- `ComputerConnection`：transport 類型（local/direct/ssh/relay）、connection identity、狀態、秘密參照；可有多種路徑通往同一個已驗證 host。
- `BotReference`：`accountId + computerId + botId`，用於 client 路由、導覽、cache 和通知。
- 每台 host 保有自己的 bots／threads 與 `rev`。既有 bot JSON 可在 client 從所屬 store 加上 computer context，不必第一步就破壞 wire format。
- 本機模式的 account namespace 為明確的 local context，不把未歸屬 host 偷偷轉成當前登入者所有。

### Store 與請求

目前 `BotStore` 只有一個 client、pairing、bots dictionary 與同步游標。要呈現跨電腦清單，建議新增 account-scoped 聚合 store，內含 `[ComputerID: BotStore]`，保留既有每 host 的 BotStore 責任。

- 聚合清單的每個 bot 都攜帶 `BotReference`，不能只拿裸 `botId` 查全域 dictionary。
- 進入聊天時把正確的 computer store 注入 `ThreadView`；send、stop、permission、history、screen、marketplace 全部沿同一條路由。
- 新 bot 的 draft 必須在選定 computer 後才填入預設值；建立命令送到該 computer。
- Push、widget、Live Activity、deep link 的新版 payload 加入穩定 computer ID；舊 payload 只有能唯一解析才導覽，否則請使用者選擇。
- 避免以 pairing token 作 UI identity；憑證輪替不應產生新的 computer。
- 不同 computer 的 bot ID、rev、session ID 即使相同，也不能混用。
- 背景／非可見電腦採有限連線與快取策略；不預設每支手機永遠開啟所有 SSE。

### 帳號切換的隔離

關閉舊帳號 streams、取消 in-flight mutations 的 UI 回傳、清除 selection／screen／terminal／pending approval，切換所有 cache namespace 和 widget snapshot。非同步回覆攜帶 account generation；切換後到達的舊回覆不得寫入新帳號狀態。

先核對目前 Clerk SDK 是否支援所需的多 session 行為。若不支援同時保留多個登入，實作「登出後切換／重新驗證」並明確呈現，不能做看似可切換但只更換畫面的假帳號清單。email 相同也不得自行合併 Clerk 使用者。

## 5. SSH 技術方案

### 首選：遠端 host + OpenSSH tunnel

遠端電腦安裝 codync-host，agent 與 repo 都在遠端；Mac 透過系統 OpenSSH 的 local forwarding 存取遠端 loopback API。SSH tunnel 本身不取代 codync-host 的授權。

```text
Mac Codync
  → 本機 loopback 上的臨時 tunnel port
  → OpenSSH 加密連線
  → 遠端 loopback 的 codync-host
  → 遠端 agent、repo、SQLite
```

不把遠端 host API 暴露到公網。採這條路可重用既有 HTTP/SSE、folder picker、agent discovery、session 持久化與 permission 流程。

第一版 SSH 範圍：

1. 使用者選擇已有 SSH config alias，或填入 hostname、port、OS username 與本機 key reference。
2. 使用系統 OpenSSH／ssh-agent，不把 SSH 私鑰上傳到 D1 或傳到手機；不自動開啟 agent forwarding。
3. 首次連線以可信管道確認 host key fingerprint；改變時阻擋並提示，不能 `StrictHostKeyChecking=no` 靜默通過。
4. tunnel 只 bind loopback，採無遠端 shell的 forwarding 模式，偵測 port 衝突、建隧道失敗、keepalive 與 reconnect。
5. 驗證遠端 host identity、protocol capabilities 與 grant 後才顯示可操作。
6. UI 分辨 SSH 驗證失敗、host 未安裝、agent 未安裝、remote cwd 不存在與 agent 尚未登入。
7. 遠端安裝／更新是獨立明確動作；不因按「測試連線」就執行安裝腳本。
8. 輸入以 structured argv 傳給 OpenSSH；限制可輸入的連線欄位，不將未驗證的 user/host/path 拼接 shell。

MVP 支援既有 key／agent 驗證與 macOS 客戶端；password、互動式 MFA、ProxyJump、複雜 ssh config 與 Linux desktop 的支援逐項驗證後宣告，不能推定全部可用。

### 為何不只把 command 改成 `ssh ... agent`

目前 ACP process 透過本機 shell 啟動，`cwd`、環境變數、MCP 設定、agent discovery 與 screen tools 都有本機假設。單純把 agent command 換成 SSH，可能讓 agent 在遠端執行，但 folder picker、工具和控制畫面仍指向本機。

因此不把這種做法當成完整的 remote computer 功能。若之後要支援「遠端不安裝 host」，需另做 ACP transport、遠端檔案／terminal callbacks、進程終止與工具轉送設計。

### 手機如何操作 SSH computer

桌面 SSH tunnel 成功不代表手機可用 Mac 的 `127.0.0.1` tunnel port。明確支援三條路徑：

| 路徑 | 條件 | 推出順序 |
|---|---|---|
| 手機直連遠端 host | 遠端 host 已配對且有 Tailscale／受保護路徑 | 先沿用現有能力 |
| 遠端 host 主動連 Cloudflare relay | 主計畫 P3 的出站中繼完成 | 推薦的後續體驗 |
| 手機經 Mac gateway，再 SSH 到遠端 | Mac 在線且明確啟用 gateway，具備逐 target 授權 | 另立功能，不含在 SSH tunnel MVP |

不將 desktop tunnel port 複製到手機配對資料。若只有 Mac 能連 SSH，手機顯示該 bot 目前需要桌面連線支援，不能把它假裝成可直接使用。

遠端 screen 依該 computer 的 OS/helper 能力決定；Linux headless computer 不顯示 Mac 的畫面控制按鈕。

## 6. 整合順序與是否現在一起做

**結論：帳號／computer／bot 的關係與 UI 導覽應一起納入現在的架構；完整 SSH 執行不是單一小改動，安排獨立的提前試行階段。**

### A. 現在納入

- 左上角使用已確認的 Codync 登入帳號語意。
- 定義一 bot 一 computer、複合路由 identity、account-scoped cache。
- 帳號切換器與 computer 管理入口分離。
- Bot 新增／設定與清單顯示 computer context。
- 帳號 SDK 整合不能假裝已完成雲端 ownership；未實作能力要如實顯示。

### B. 多 computer 路由

- 新增 account 聚合 store，重用每 computer 的 BotStore。
- 改寫 navigation、push、widget、screen 與 marketplace 路由。
- 測試同帳號兩台電腦、相同 bot ID、其中一台離線與帳號切換競態。

### C. SSH 桌面試行，可早於雲端中繼

- 先以測試用遠端 host 完成 tunnel、host key 驗證、連線生命週期與身份驗證。
- 確认 folder picker／agent 安裝／chat／stop／permission 都在遠端。
- 只有在 B 的路由和授權驗收通過後才整合到正式 bot 建立流程。
- 無可用 SSH 測試環境時可以做本機 mock／loopback 整合測試，但不能宣告真實跨機器流程驗收完成。

### D. 手機遠端體驗

優先讓遠端 host 直接接 Cloudflare 中繼。Mac gateway 與手機原生 SSH client 都增加另一套生命週期與金鑰管理，等到有確實需求再做。

## 7. 新增驗收條件

- 同帳號的本機 bot 與 SSH bot 可以並列，send／stop 不會送錯 computer。
- 兩台 computer 同名或具有相同 bot ID，仍能正確區分。
- 切換帳號後，不殘留上一個帳號的聊天、權限卡、用量、terminal 或 widget。
- 遠端 agent、專案目錄、MCP、skills、用量都來自遠端 host。
- SSH 斷線不會在本機重跑命令；連回後依遠端狀態恢復，不重複建立 task。
- Remote host key 變更、授權失效、OS user 變更時阻擋並重新驗證。
- 移除 connection profile 不刪除遠端 repo／SQLite；撤權與刪除 bot 是各自明確的操作。
- 停止一個 bot 不終止其他 bot 共用的 SSH tunnel；只有沒有使用者／引用時才釋放連線。
- 手機不使用桌面的 loopback 位址；不支援的 screen／terminal 能力正確隱藏或提示。
- SSH profile／device credentials 不進 D1、log 或一般 UserDefaults；只保存必要的非秘密描述與安全儲存參照。

## 8. 目前評估依據

對照 `apps/ios/Views/BotListView.swift`、`SettingsView.swift`、`kit/Sources/CodyncUI/Store/BotStore.swift`、`HostClient.swift`、`host/src/acp.rs` 與 `apps/macos/App/HostController.swift`：目前是一個 active computer 對應一個 store／client 的設計，尚無跨 computer 的 account store 或 SSH 連線管理。因此此功能不能只換手機左上角圖示或加 `computerId` 就算完成。

SSH 與 Clerk 具體 API／平台相容性應在實作階段依當時官方文件和已安裝版本核對；本文件未宣告任何 SSH 或多帳號功能已經可用。

## 9. 本次已落地與仍在規劃的範圍

已落地：iOS 左上角帳號入口、Clerk Google 登入／新增帳號／session 切換、每個帳號獨立的手機配對清單與聊天 cache、切換時退出舊 store 和結束 Live Activities。「Computers & settings」移到帳號選單內。單 session 的 Clerk instance 會顯示登出後換帳號，而不是假的多帳號切換。

這是手機端的登入與本機資料分區，尚未建立 Cloudflare ownership、雲端裝置目錄或逐裝置撤權。每個帳號仍需手動配對 computer；同一份有效配對憑證若被再次提供給另一帳號，現有 host 仍可能接受。舊 APNs ticket 也尚未具備帳號級撤權，相關安全邊界必須依主計畫 P2 完成後才能宣稱具備後端帳號隔離。

跨 computer bot 彙整、新 bot 選 computer、SSH transport、手機 SSH gateway 與雲端中繼仍屬後續工作。現在清單顯示目前所選 computer 的 bots，每個 bot 的執行位置仍由其 host 固定。
