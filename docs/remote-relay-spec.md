# 遠端連線實作規格：Cloudflare 中繼（主要）、直連（替代）、帳號與 SSH

日期：2026-09-25（rev 2：安全與可實作性審查後修訂）
狀態：**實作規格**，交給平行的 CLOUD／HOST／APPLE-KIT／APPLE-APPS 四條 track。與 [帳號與裝置計畫](cloudflare-account-device-plan.md)、[SSH 計畫](bot-computer-ssh-plan.md) 衝突時，以本文件為準。
測試向量：[remote-relay-vectors.json](remote-relay-vectors.json)（由 [remote-relay-vectors.py](remote-relay-vectors.py) 以固定種子產生，`python3` + `cryptography`；三種語言的實作都必須逐欄通過）。

本文件中「必須／不得」是介面契約；沒有寫到的內部結構由各 track 自行決定。各 track 之間**只透過本文件溝通**，發現本文件有矛盾或缺漏時停下回報，不要自行發明另一套 wire format。

---

## 0. 決策摘要

| # | 決策 | 理由 |
|---|---|---|
| D1 | **Cloudflare 中繼是離家連線的主要路徑**；LAN／Tailscale 直連是替代：client 先平行試直連（1.5 秒內完成握手就用），否則走中繼。Tailscale 永遠不是必要條件。 | 使用者不用裝 VPN、不用開 port。 |
| D2 | **端對端加密（locked box）**。直連和中繼**用同一套 channel 協定**：Ed25519 簽章的臨時 X25519 ECDH → HKDF-SHA256 → ChaCha20-Poly1305。Worker／DO 只看得到密文與路由 metadata。 | 一套協定、一份測試向量；LAN 上也不再有明文 token。 |
| D3 | 每台 computer 一個 Durable Object（SQLite storage、WebSocket Hibernation）：presence、轉送密文、離線 mailbox（手機→電腦，24h TTL，可取消，依序投遞，ack 後刪除）。電腦→手機的補送仍由 host 的 `rev`／events 語意負責。 | 符合產品決策；DO 不保存聊天。 |
| D4 | **Host 是授權的唯一權威**。host 在本機 SQLite 保存「已授權裝置」表；**只有兩條路能新增列**：本機 QR 配對（§4.1）與 host 自己的核准動作（`decideAccessRequest` approve，§4.2 B）。雲端 state **只能縮短**授權（刪除列、延長既有列的 lease），永遠不能新增。host 把此表簽成 **ACL** 發佈到自己的 DO，DO 據此放行 WebSocket；E2E 握手時 host 再驗一次。 | 無帳號模式也能用中繼（host 以機器金鑰向雲端註冊，不需帳號）；雲端被攻破也無法新增可解密的裝置（新增要 host 本機的使用者動作、ACL 要 host 簽、握手要 host 私鑰）；撤權有兩道即時關卡（DO 封鎖 + host 關 channel）。 |
| D5 | **Computer ID 由 host 簽章公鑰推導**：`computerId = b64url(SHA-256(hostSignPub)[0..16])`。不由雲端分配。 | 自我驗證、離線可得、無法被搶註；DO 名稱可直接推導。代價：換 host 金鑰 = 新 computer（見 §3.5）。 |
| D6 | 帳號 → Computers → Bots：新增 `AccountStore`（`[ComputerID: BotStore]`），`BotReference = accountId + computerId + botId`。 | 產品決策 5。 |
| D7 | 共用 bearer token（`~/.codync/token`）**只接受 loopback 連線**（Mac App 本機、SSH tunnel、statusline、MCP、TUI、Linux App）。手機與其他遠端 client 一律走 E2E channel。 | 移除「一個 token 開所有手機」的舊模型；不留相容路徑。 |
| D8 | SSH：macOS App 以系統 OpenSSH `-L` 轉發到遠端 host 的 loopback API；遠端側看到的是 loopback 呼叫者，所以 SSH 帳號等同本機使用者權限（能讀 `~/.codync/token` 的人本來就有這權限）。 | 產品決策 6；重用 loopback API，不需要額外授權層。 |
| D9 | 版本升為 **3.0.0**（`MARKETING_VERSION`、`host/Cargo.toml`）：手機↔host 協定不相容（無 token 直連），依 CLAUDE.md 規則升 major。 | 2.x App 不會誤連 3.x host。 |
| D10 | **推播內容也加密**：host 以裝置註冊的 X25519 push key 封裝通知標題／內文（§6.7），APNs 只帶通用文字 + `mutable-content`，iOS Notification Service Extension 解開。Live Activity 只推狀態 enum，不推自由文字。 | 決策 2：Cloudflare（`relay/`）只看得到密文。 |

**審查後的取捨（rev 2）**：
- 撤權後的 DO 解封鎖以 **grantId** 判斷（新核准才解），不比較時間戳（見 §7.3）；比 `stateAt` 方案少一個跨時鐘欄位。
- Mailbox 不在 host 端追蹤已見過的 `epk`：重用 epk 的洩漏發生在送出端、密文已在雲端，host 拒收也無法補救；改以 client 端 MUST（§6.4）與測試保證。
- SSH 目標設了 `ProxyJump`／`ProxyCommand` 時不做 keyscan、也不做 `SSH_ASKPASS` 首次確認：要求使用者先在終端機連過一次（host key 已在 `~/.ssh/known_hosts`）。少見情境，換來不必實作 askpass helper。
- 推播選擇「加密 + NSE」而非「只送通用文字」：保留 Grok Bot 式的通知預覽。

明確不做（這一版）：雲端保存聊天、Mac 當手機的 SSH gateway、手機原生 SSH、TURN、host 金鑰輪替後沿用舊授權、團隊共享。

---

## 1. 名詞與 ID

| 名稱 | 格式 | 說明 |
|---|---|---|
| `hostSignPub` / `signKey` | 32 bytes Ed25519 公鑰，b64url | host 身分金鑰 |
| `hostBoxPub` / `boxKey` | 32 bytes X25519 公鑰，b64url | 只用於 mailbox 封裝 |
| `computerId` | `b64url(SHA-256(hostSignPub)[0..16])`，22 字元 | computer 的穩定 ID（D1 `computers.id`、DO 名稱、Apple `ComputerID`） |
| `hostId` | 既有 UUID（`store.kv host_id`） | **意義不變**：本機資料庫身分，client 用來偵測「換了一個資料庫要重置 mirror」 |
| `deviceKey` / `dk` | 32 bytes Ed25519 公鑰，b64url | 每個 App 安裝 × 每個帳號 context 一把 |
| `clientNonce` | 既有 UUID 字串 | `send` 去重；mailbox 的冪等鍵 |
| `grantId`、`requestId`、`claimId`、`deviceId` | `grt_`/`req_`/`clm_`/`dev_` + b64url(16 random bytes) | D1 資源 ID |
| `link` | b64url(12 random bytes) | DO 為每條 device WebSocket 指派，host 端用來多工 |
| `pushKey` | 32 bytes X25519 公鑰，b64url | 每個 App 安裝 × 帳號 context 一把，只用於推播封裝（§6.7） |

編碼規則（**只適用於本規格新定義的欄位**：金鑰、簽章、nonce、`d`、blob、ID、ACL、QR 參數）：二進位一律 **base64url、無 padding**（RFC 4648 §5）。時間一律 **Unix epoch 毫秒整數**（JSON number）。字串比對大小寫敏感。

**Inner RPC 的 body 與事件 payload 逐位元組等於今日 HTTP／SSE 的 JSON**，不套用上述規則（例如 `termInput.data`、term `output.data` 維持標準 padded base64；screen SDP 維持原格式）。

命名：`cloud` 一律指 Cloudflare 帳號／中繼服務的 base URL（QR `cloud=`、`hello.cloud`、`Computer.cloud`）；`relay` 一律指既有 APNs 推播 relay（`registerDevice.relay`、kv `relay_url`）。

---

## 2. Track 分工與檔案所有權

| Track | 獨占路徑 | 產出 |
|---|---|---|
| **CLOUD** | `cloud/**`（新建）、`relay/**`（只改 §6.7 的 `mutableContent` 透傳）、`.github/workflows/cloud.yml`（新建） | Worker（`/v1` API + Clerk + D1）、`ComputerRelay` DO、migrations、vitest、e2e 測試、`cloud/README.md`、CI |
| **HOST** | `host/**`、`.github/workflows/host.yml` | identity、channel、relay client、cloud client、逐裝置授權、loopback-only token、CLI |
| **APPLE-KIT** | `kit/**`、`.github/workflows/kit.yml`（新建，`swift test`） | `DeviceIdentity`、channel 加密、`HostTransport`、`HostConnector`、`CloudClient`、`AccountStore`、`BotStore` 調整、models |
| **APPLE-APPS** | `apps/ios/**`（含 `apps/ios/Widgets`、新 `apps/ios/NotificationService`）、`apps/macos/**`、`apps/shared/**`、`project.yml`、`Codync.xcodeproj`（只經 `xcodegen generate`）、`docs/clerk-macos.md`、`.github/workflows/release-macos.yml` | UI、帳號流程、Mac 核准介面、SSH、Notification Service Extension、entitlements、設定檔 |

- `relay/`（APNs 推播）只加一個欄位：`/push` body 的 `mutableContent: true` → `aps["mutable-content"] = 1`。其他不變。推播撤權由 host 端「ticket 綁定 deviceKey、撤權即刪」達成（§9.4）。
- `docs/structure.md` 的 `cloud/` 一行已由本規格作者加入。
- `apps/linux/**`、`apps/screen*/**`：不動（它們只用 loopback token / unix socket）。
- 共用唯讀檔：`docs/remote-relay-spec.md`、`docs/remote-relay-vectors.json`。任何 track 要改它們 → 回報 orchestrator，不自行修改。
- `CLAUDE.md`、`docs/cloudflare-account-device-plan.md`、`docs/bot-computer-ssh-plan.md`：由本規格作者更新；各 track 不改。
- 版本：HOST 把 `host/Cargo.toml` 的 `version` 設為 `3.0.0`；APPLE-APPS 把 `project.yml` 的 `MARKETING_VERSION` 設為 `3.0.0`（`CURRENT_PROJECT_VERSION` 只在上傳時遞增）。

**順序**：四條 track 同時開始，各自以 `remote-relay-vectors.json` 做單元測試，互不等待。APPLE-APPS 依賴 APPLE-KIT 的公開 API（§10.1 已定死簽章），可先以本文件寫 UI，kit 完成後接上。跨 track 整合測試（§12.1）由 CLOUD 擁有，在 HOST 合併後執行。

---

## 3. 身分與金鑰

### 3.1 Host

- 首次啟動產生兩把金鑰：Ed25519 `sign`、X25519 `box`（兩者獨立，不互相轉換）。
- 存於 `data_dir()/identity.json`，權限 `0600`，**不放 SQLite**（避免複製資料庫就複製身分）：
  ```json
  {"v":1,"sign":"<b64url 32-byte seed>","box":"<b64url 32-byte scalar>"}
  ```
- 寫入：先寫 `identity.json.tmp`（0600）再 rename。讀取失敗（格式錯）→ 啟動失敗並說明，不自動覆蓋。
- `computerId` 由 `sign` 公鑰推導（§1）。

### 3.2 Device（iPhone、Mac 作為 client）

- 只有一把 Ed25519 金鑰（`Curve25519.Signing.PrivateKey`）。
- 每個 `SharedStore.Context.id`（帳號 namespace，`local` 或 hash(userID)）一把，避免跨帳號關聯。
- 另有一把 X25519 **push key**（`Curve25519.KeyAgreement.PrivateKey`），同樣每 context 一把，只用於解開推播（§6.7）。
- Keychain：`kSecClassGenericPassword`，`kSecAttrService = "com.pokai.Codync.device-key"`（push key 用 `"com.pokai.Codync.push-key"`），`kSecAttrAccount = context.id`，值 = 32-byte `rawRepresentation`，`kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`，`kSecAttrSynchronizable = false`。
- macOS：**不設** `kSecUseDataProtectionKeychain`（Mac App 是 Developer ID、未 sandbox、沒有 entitlements 檔；data protection keychain 需要 provisioning profile 背書的 entitlement，否則 `-34018`）。使用 login keychain，不設 access group。
- iOS App、Widgets、NotificationService 共用 access group `$(AppIdentifierPrefix)com.pokai.Codync`（APPLE-APPS 在 `project.yml` 對這三個 target 加 `keychain-access-groups` entitlement；kit 從 Info.plist `CodyncKeychainGroup` 讀 group，缺值則不設 group）。
- 私鑰永不離開 Keychain／process；雲端只存公鑰。
- 帳號登出／移除帳號：刪除該 context 的 device key、computers 快取、mailbox 草稿、widget snapshot。

### 3.3 Per-(device, host) E2E 金鑰

沒有長期共用對稱金鑰。**每次 channel 連線**做一次 signed-ephemeral ECDH（§6.2），得到兩個方向的 32-byte 金鑰；連線結束即丟棄（前向保密）。雙方的長期身分金鑰是 pin 住的公鑰：

| 流程 | device 如何得知 host 公鑰 | host 如何得知 device 公鑰 |
|---|---|---|
| QR（本機、無帳號） | QR 內 `sk`／`bk`（可信的實體通道） | 配對時在 E2E channel 內以一次性 pairing code 證明（§4.1） |
| 帳號 + 存取申請 | `GET /v1/computers` 取 `signKey`（雲端傳遞）＋**commit-then-reveal SAS 6 碼比對**防止雲端換鑰（§4.2） | D1 access request → host 在電腦上由使用者核准（同一組 SAS） |

`boxKey`（mailbox 用）**只從已驗證來源 pin**：QR 的 `bk`，或 E2E channel 內 inner `hello` 回應的 `boxKey`。雲端回傳的 `boxKey` 只作顯示，不得用於封裝。尚未 pin `boxKey` 的 computer 不提供 mailbox（帳號流程的手機在核准後第一次連線即取得，因為核准當下 host 必定在線）。

### 3.4 撤權

| 動作 | 效果 |
|---|---|
| 在電腦上撤銷裝置（Mac UI／`codync-host devices revoke`） | host 刪除裝置列與其 push tickets、Live Activity tickets → 立即關閉該裝置所有 channel → 發佈新 ACL → DO 關閉其 WebSocket；帳號來源的另呼叫 `DELETE` 對應 grant（見 §8.4 `POST /v1/host/grants/{id}/revoke`） |
| 雲端撤銷 grant／裝置／解除綁定／刪帳號 | Worker 寫 D1 → 呼叫 DO `/internal/block`（`[{dk, grantId}]`，立即關閉並封鎖）→ DO 通知 host `cloud.changed` → host 拉 `/v1/host/state` 並刪除裝置列 |
| host 連不到雲端 | 帳號來源裝置的 lease 最長 15 分鐘後失效，host 關閉 channel；本機（QR）來源裝置不受影響 |

### 3.5 輪替

- **Channel 金鑰**：每次連線新產生；任一方向 counter 達 2^32 或連線滿 24 小時 → host 以 close code `4011 rekey` 關閉，client 立即重連。
- **Device key**：不做原地輪替。要換 → 刪 key、重新配對／重新申請。
- **Host key**：不做原地輪替。刪除 `identity.json` 即成為新 computer（新 `computerId`），所有裝置需重新配對；舊 D1 列由使用者在帳號中移除。這是刻意的簡化（D5）。
- **Loopback token**：`codync-host reset-token` 照舊，只影響本機 helper。

---

## 4. 配對與授權流程

### 4.1 QR 配對（無帳號也可用）

QR／連結保留 `codync://pair` scheme，**移除 `token`**，欄位：

```
codync://pair?v=3&name=<pct>&id=<computerId>&sk=<signPub>&bk=<boxPub>&code=<pairing code>&urls=<pct a,b>&cloud=<pct cloud base URL>
```

| 欄位 | 必填 | 說明 |
|---|---|---|
| `v` | 是 | 固定 `3`；缺或不同 → App 顯示「請更新電腦上的 Codync」 |
| `id` | 是 | 必須等於 `computerId(sk)`，否則拒絕 |
| `sk`, `bk` | 是 | host 公鑰 |
| `code` | 是 | 16 random bytes，b64url；一次性、10 分鐘；host 最多同時 5 組（新的擠掉最舊的） |
| `urls` | 否 | 直連候選（既有 `service::addresses`，`http://ip:port` 形式），可為空 |
| `cloud` | 否 | host 使用的 cloud base URL（`https://…`；DEBUG build 另允許 `http://127.0.0.1:*`／`http://localhost:*`）；缺 = host 關閉雲端 |
| `name` | 否 | 顯示名稱 |

`urls` 與 `cloud` 至少要有一個。`offerId = b64url(SHA-256("codync/offer/v1" ‖ codeBytes)[0..16])`。

流程：
1. 使用者在電腦上開 QR（Mac 選單 → loopback `pairing`；或 `codync-host pair`）。host 產生 code，記在記憶體，並立即發佈含 `offers` 的新 ACL。
2. 手機載入／建立 device key，用 §7.5 的連線策略連線，hello 帶 `"pair": true`（中繼另需 URL `pair=<offerId>`）。
3. 握手成功（手機已用 QR 的 `sk` 驗證 host）後，第一個 request 必須是：
   ```json
   {"id":1,"m":"pair","b":{"code":"<code>","name":"Kevin's iPhone","platform":"ios"}}
   ```
4. host 驗 code（常數時間比較、未過期、未用過）→ 刪除該 code → 新增裝置列（`source=local`，`grant_id=NULL`，`scopes=["control","screen"]`，無 lease）→ 回 `{"id":1,"ok":{"computerId":"…","name":"…"}}` → **關閉這條 channel，close code `4100 paired`**（中繼：host 依序送 `data`（回應）→ `close {link, code:4100}` → 新 ACL（移除 offer、加入 dk）；同一條 host socket 保序，所以回應一定先到）。
5. client 收到 `4100` 立即以一般模式重連（§7.5，不帶 `pair`）。直連與中繼行為相同；DO 不需要「升級」pairing socket。

`Caller::Pairing` 的 channel 只接受 `pair`：第一個 inner 訊息不是 `pair`、或 `pair` 之後還有任何訊息 → 關閉 `4002 protocolError`。

錯誤與限流：
- 沒有任何未過期的 pairing code 時，`pair:true` 的 `hello` 在做 ECDH／簽章驗證**之前**就回 `reject pairingClosed` 並關閉。
- 限流只計算「到達 `pair` request」的嘗試：每個來源（直連：peer IP；中繼：link）每分鐘 3 次，全 host 每分鐘 10 次；超過回 `reject rateLimited`。
- code 錯／過期 → `{"err":{"status":403,"message":"This pairing code expired. Show a new one on the computer."}}` 並關閉 channel。

### 4.2 帳號流程

**A. 電腦加入帳號（claim）**—在該電腦上的 Mac App（或經 SSH tunnel 的 Mac App）操作：
1. Mac App（Clerk 已登入）`POST /v1/claims` → `{claimId, nonce, expiresAt}`（5 分鐘）。
2. Mac App 呼叫 loopback `claimSign {claimId, nonce, userId}` → host 回傳註冊資料與簽章：
   `sig = Ed25519_host("codync/claim/v1\n" + claimId + "\n" + nonce + "\n" + userId + "\n" + computerId + "\n" + boxKey)`（UTF-8，**無結尾換行**；向量 `claim`）。Worker 以此 `boxKey` 建立或更新 `computers.box_pub`，不接受 client 另給的值。
3. Mac App `POST /v1/claims/{claimId}/complete`（Clerk）帶上 host 回傳的內容。Worker 驗 Clerk、claim、簽章，原子化設定 owner。
4. Worker 通知 DO `cloud.changed` → host 拉 state 得知 owner。

**B. 手機取得存取權**（commit-then-reveal SAS，ZRTP／BLE numeric comparison 同型）：
1. 手機登入後 `POST /v1/devices`（Clerk + 該 device key 的 `Codync-Sig`）。
2. `GET /v1/computers`（帶 `Codync-Sig` 則回傳每台的 `access`）。
3. 對 `access == "none"` 的電腦：手機產生 32 random bytes `nD`（只存記憶體），`POST /v1/computers/{id}/access-requests {"commit": b64url(SHA-256("codync/sascommit/v1" ‖ dk ‖ nD))}` → `{requestId, expiresAt}`。
4. Worker 通知 DO → host 拉 state → 看到新申請（含 `commit`）→ host 產生 32 random bytes `nH`（存本機記憶體，每個 requestId 只產生一次）→ `POST /v1/host/access-requests/{id}/nonce {"nonce": nH}`（已有 host nonce → `409 conflict`，host 不得換）。
5. 手機輪詢 `GET /v1/access-requests/{id}`（每 2 秒）→ 出現 `hostNonce` 後才 `POST /v1/access-requests/{id}/reveal {"nonce": nD}` → Worker 存入並通知 DO `cloud.changed`。手機此時計算 SAS 顯示「在電腦上確認代碼 123456」。
6. host 拉 state 取得 `deviceNonce` → 驗 `SHA-256("codync/sascommit/v1" ‖ dk ‖ nD) == commit`（不符 → 自動 deny 並記 `warn`）→ 計算 SAS → `accessRequests` 事件 → Mac App 顯示核准對話框（裝置名稱、平台、帳號 email、SAS）。**`nD` 驗證通過前 host 不顯示 SAS、不接受 approve**（`decideAccessRequest` 回 409）。無 GUI 的 host：`codync-host access list|approve|deny`。
7. 核准 → host `POST /v1/host/access-requests/{id}/decision {"decision":"approve"}` → D1 建立 grant → 回 `grantId` → host **以自己顯示給使用者的 `dk` 與回傳的 `grantId`** 新增裝置列（`source=account`，lease 15 分鐘）→ 發佈 ACL。
8. 手機輪詢到 `approved` → 連線（§7.5）。第一次 `hello` 回應帶 `boxKey`，此時 pin（§3.3）。

**SAS**：`sas = uint32_be(SHA-256("codync/sas/v2" ‖ hostSignPub ‖ dk ‖ nD ‖ nH)[0..4]) mod 1_000_000`，左補零成 6 位數（向量 `sas`）。手機用從雲端拿到的 `signKey` 算、host 用自己的公鑰與申請內 dk 算。因為雙方的 nonce 在對方的值確定之後才揭露，被攻破的雲端每次申請只有 10⁻⁶ 的機會讓兩邊代碼相同，無法離線搜尋金鑰。手機核准後 pin 住算 SAS 用的那把 `signKey`，之後以 `welcome.sig` 驗證。

### 4.3 授權表與 ACL（host 為權威）

host 的授權裝置表（§9.2）每一列：`key, name, platform, source(local|account), grant_id, scopes, lease_until`。

規則（**雲端只能縮短授權，不能新增**）：
- 新增列只有兩條路：§4.1 的 `pair`（`source=local`，`grant_id=NULL`）與 §4.2 B 的 host 核准（`source=account`，`grant_id` = decision 回傳值，`key` = host 顯示給使用者的 dk）。
- 套用 `/v1/host/state` 時，只看本機已有的 `source=account` 列：`(grant_id, key)` 出現在 `grants` → `lease_until = now + 15min`；不在 → 刪除（同 transaction 刪 push／activity tickets）。
- state 裡本機不認識的 grant（或 `grant_id` 相同但 `deviceKey` 不同）→ 忽略並 `tracing::warn!(grant_id, "unknown grant from cloud ignored")`；不新增、不改寫任何列。`source=local` 的列不受 state 影響。
- 狀態拉取成功時才延長 lease；連不到雲端就讓 lease 自然到期。
- `lease_until` 過期的列視同不存在（不必立即刪；下次拉取時清）。
- 每次連上 relay（含重連）：先完成一次 state 拉取（最多等 10 秒）才送 `acl`／`ready`；拉取成功之前，`source=account` 的列在直連與中繼上都不接受握手。
- 表有任何變動 → `acl_ver` 前進（§7.3 規則）→ 發佈 ACL。

ACL（host 簽章，DO 驗證；逐位元組簽章，不做 JSON 正規化；向量 `acl`）：
```json
{"v":1,"computerId":"…","ver":12,
 "devices":[{"dk":"…","grant":"grt_…"}, {"dk":"…","grant":null}],
 "offers":[{"id":"<offerId>","exp":1790000600000}]}
```
`grant: null` = 本機配對。ACL 不帶 lease 與 scopes：lease 與 scope 由 host 在握手與每個 request 時判斷，DO 只做「是否列在最新 ACL、是否被封鎖」的粗篩（§7.1）。發佈格式見 §7.3 `acl`。

---

## 5. Codync-Sig：HTTP／WebSocket 請求簽章

用於：host → `/v1/host/*`、host／device → `/v1/relay/*`（WebSocket upgrade）、device 在 `/v1/devices`、`/v1/computers/{id}/access-requests`、`/v1/access-requests/{id}/reveal` 與 `GET /v1/computers` 的持有證明。

Header：
```
Codync-Sig: v=1,kid=<b64url pub>,ts=<ms>,nonce=<b64url 16 random bytes>,sig=<b64url 64-byte sig>
```
簽章輸入（UTF-8，`\n` 分隔，無結尾換行）：
```
codync-sig-v1
<METHOD 大寫>
<authority：小寫的 Host header 值，含非預設 port，例如 codync-cloud-staging.x.workers.dev 或 127.0.0.1:8787>
<path + query，與實際送出的 request-target 逐字相同>
<ts>
<nonce>
<b64url(SHA-256(body bytes))>   ← 無 body（GET、WS）用空字串的 hash
```
驗證：`|now - ts| ≤ 300_000`；`(kid, nonce)` 在 10 分鐘內未出現過（HTTP：D1 `sig_nonces`；relay：DO 的 `nonces` 表）；Ed25519 驗證（Worker 用 WebCrypto `Ed25519`）。Worker 以 `new URL(request.url).host`（小寫）比對 authority，所以 staging 簽的 header 在 production 無效。失敗 → `401 {"error":{"code":"badSignature"}}`。向量：`requestSig`。

---

## 6. 密碼學規格（channel、mailbox）

所有向量在 `remote-relay-vectors.json`。`‖` = 位元組串接；ASCII 標籤不含結尾 NUL。

### 6.1 Channel 訊息（明文 JSON，WebSocket text frame）

| `t` | 方向 | 欄位 |
|---|---|---|
| `hello` | device→host | `v`=1, `dk`, `ek`（device 臨時 X25519 公鑰）, `n`（32 random bytes）, `sig`, `pair`（bool，選填） |
| `welcome` | host→device | `v`=1, `ek`（host 臨時 X25519 公鑰）, `sig` |
| `reject` | host→device | `code`（`unauthorized`/`revoked`/`leaseExpired`/`badSignature`/`unsupportedVersion`/`pairingClosed`/`rateLimited`）, `message` |
| `f` | 雙向 | `c`（u64 counter，JSON number）, `d`（b64url 密文+tag） |

### 6.2 握手

```
cidRaw = SHA-256(hostSignPub)[0..16]
hs1 輸入 = "codync/hs1/v1" ‖ cidRaw ‖ dk ‖ ekD ‖ n          (13+16+32+32+32 bytes)
hello.sig = Ed25519_device(hs1 輸入)
TH        = SHA-256("codync/hs2/v1" ‖ cidRaw ‖ dk ‖ ekD ‖ n ‖ ekH)
welcome.sig = Ed25519_host(TH)
ss   = X25519(ekD_priv, ekH)      ← 全零結果必須拒絕
prk  = HKDF-Extract(salt = TH, ikm = ss)
kD2H = HKDF-Expand(prk, "codync/d2h/v1", 32)
kH2D = HKDF-Expand(prk, "codync/h2d/v1", 32)
```
- host 的檢查順序（便宜的先做）：
  1. 中繼 link：`hello.dk` 必須等於 DO 在 `open` 給的 `dk`；`pair` 必須與 `open.pair` 相同。不符 → `4002`。
  2. `pair == true`：沒有未過期的 pairing code → `reject pairingClosed`（不做任何密碼運算）；限流見 §4.1。
  3. `pair != true`：`dk` 必須是有效授權（§4.3，含「relay 連上後 state 已拉取」條件）。否則 `reject unauthorized|revoked|leaseExpired`。
  4. 驗 `hello.sig` → 產生 `ekH` → `welcome`。
- 裝置的 `connected` 與 `last_seen_at` **只在收到第一個成功解密的 `f` 之後**才更新（重放的 `hello` 無法推導金鑰，不得讓裝置看起來在線）。
- device：用 pin 住的 `hostSignPub` 驗 `welcome.sig`；失敗 → 視為攻擊，關閉並回報 `.unauthorized("This computer's identity changed…")`，**不**改走其他路徑重試同一 host key。
- 握手期限：host 收到連線後 10 秒內沒有 `hello` 就關閉；device 送出 `hello` 後 10 秒內沒 `welcome` 就放棄。

### 6.3 Frame 加密

```
nonce(12) = 0x00000000 ‖ u64_be(c)
aad       = "codync/frame/v1" ‖ u64_be(c)
plaintext = flag(1 byte: 0x00 最後一段, 0x01 還有後續) ‖ chunk bytes
d         = ChaCha20-Poly1305(kDir, nonce, plaintext, aad)   (密文 ‖ 16-byte tag)
```
- 每個方向各自從 `c = 0` 起算，每 frame +1；接收方要求 `c` 恰為預期值，否則關閉（close `4002 protocolError`）。這同時提供重放與重排保護。
- 一則 inner 訊息（§6.5 的 JSON）UTF-8 編碼後切成 ≤ 256 KiB 的 chunk；接收方串接直到 flag `0x00`，再 JSON parse。
- 重組上限：device→host 1 MiB；host→device 16 MiB。超過 → 關閉 `4013 tooLarge`。

### 6.4 Mailbox 封裝（device → 離線 host）

```
epk, eprv = 新的 X25519 金鑰對
ss   = X25519(eprv, hostBoxPub)          ← 全零拒絕
prk  = HKDF-Extract(salt = "codync/mbox/v1" ‖ cidRaw ‖ dk ‖ epk, ikm = ss)
key  = HKDF-Expand(prk, "codync/mbox-key/v1", 32)
ct   = ChaCha20-Poly1305(key, nonce = 12 × 0x00, plaintext = inner JSON, aad = dk ‖ UTF-8(clientNonce))
sig  = Ed25519_device("codync/mbox/v1" ‖ cidRaw ‖ epk ‖ ct)
blob = epk(32) ‖ sig(64) ‖ ct
```
固定 nonce 安全的前提：**每次封裝都必須產生新的 `epk`（MUST）**，不得快取或重用。重試時只能重送當初存下的同一個 blob，或以新的 `epk` 重新封裝。kit 與 TS 參考實作各有一個測試：連續封裝兩次同一則訊息，`epk` 不同。inner JSON：
```json
{"m":"send","b":{"botId":"…","text":"…","clientNonce":"<同 envelope>"},"ts":1790000000000}
```
`hostBoxPub` 只能用 §3.3 pin 住的值。host 檢查：dk 目前有效 → 驗 sig → 解密（aad 用 envelope 的 `from` 與 `nonce`）→ `b.clientNonce == nonce` → `now - ts ≤ 25h` → `m` 只允許 `send` → 走一般 `send` dispatch（既有 `find_by_nonce` 去重）。

### 6.5 Inner RPC（加密 frame 內的 JSON）

對應既有 `POST /api/<method>` 與 `GET /events`、`GET /term/{id}`：

| 訊息 | 方向 | 形式 |
|---|---|---|
| request | d→h | `{"id":N,"m":"<method>","b":<body 或 null>}` |
| 成功 | h→d | `{"id":N,"ok":<與 HTTP 回應 body 相同的 JSON>}` |
| 失敗 | h→d | `{"id":N,"err":{"status":<HTTP 狀態碼>,"message":"…"}}`（status 對應今日 `ApiError`：400/403/404/500） |
| 訂閱 | d→h | `{"id":N,"sub":"events","b":{"since":<rev>,"client":"ios"|"mac"}}` 或 `{"id":N,"sub":"term","b":{"term":"<id>"}}` |
| 串流事件 | h→d | `{"id":N,"ev":<與今日 SSE `data:` 完全相同的 JSON>}` |
| 串流結束 | h→d | `{"id":N,"end":true}`（term exit 之後）或 `err` |
| 取消 | d→h | `{"id":N,"cancel":true}`：訂閱 → host 停止串流並回 `end`；一般 request → host 丟棄回應（**不中止已開始的 mutation**） |

- `id`：device 端遞增 u32，連線內唯一。
- 每條 channel 最多 32 個進行中的 request、4 個訂閱；超過回 `err 429`。
- `events` 訂閱的語意與 SSE 完全相同（先 subscribe、再 catch-up、`hello` 在最前、落後時送 `{"type":"resync"}`）；`client == "ios"` 時計入 `hub.ios_clients`（推播抑制）。
- 串流沒有額外的 keep-alive；存活由 WebSocket ping（§7.6）負責。

### 6.6 方法權限（host 以 `Caller` 判斷）

| 類別 | 方法 | 允許的 caller |
|---|---|---|
| loopback 專用 | `setScreenEnabled`, `pairing`, `computerCall`, `claimSign`, `unclaim`, `devices`, `revokeDevice`, `accessRequests`, `decideAccessRequest`, `cloudStatus`, `setCloud` | `Caller::Local`（loopback + token） |
| 配對中 | `pair` | 只在 pairing 狀態的 channel；配對完成後不可再呼叫 |
| screen scope | `screenOffer`, `screenClose`, `screenTakeover` | Local，或 scopes 含 `screen` 的裝置 |
| control scope | 其餘所有既有方法（`hello`, `sync`, `history`, `createBot`, `updateBot`, `deleteBot`, `markRead`, `send`, `stop`, `newSession`, `respondPermission`, `registerDevice`, `registerActivity`, `refreshBackends`, `usage`, `listDirs`, market／skills／connectors、`agentSetup`, `agentAuth`, `agentAuthenticate`, `setAgentEnv`, `termInput`, `termResize`, `termClose`, `screenStatus`） | Local，或 scopes 含 `control` 的裝置 |

v1 發出的 grant 一律 `["control","screen"]`。

### 6.7 推播封裝（host → APNs → 手機）

裝置在 `registerDevice` 帶 `pushKey`（§1）。host 對每個 ticket：
```
epk, eprv = 新的 X25519 金鑰對（每則通知、每個 ticket 各自新產生）
ss   = X25519(eprv, pushKey)            ← 全零拒絕
prk  = HKDF-Extract(salt = "codync/push/v1" ‖ cidRaw ‖ pushKey ‖ epk, ikm = ss)
key  = HKDF-Expand(prk, "codync/push-key/v1", 32)
ct   = ChaCha20-Poly1305(key, nonce = 12 × 0x00, plaintext = {"title","body"} JSON, aad = cidRaw)
sealed = b64url(epk ‖ ct)
```
送給 `relay/` 的 body：`alert = {"title":"Codync","body":"Needs you"|"Done"}`（通用文字，依 kind）、`mutableContent: true`、`threadId = botId`、`category = kind`、`data = {"botId","computerId","ctx":<SharedStore.Context.id>,"sealed"}`。Notification Service Extension 依 `ctx` 從共用 Keychain 取 push key、解開後替換 title／body；解不開就保留通用文字。Live Activity 的 `contentState` 只含 `status` 與 `startedAt`（`activity` 固定為空字串），不含任何自由文字。向量 `push`。

---

## 7. Relay wire protocol（Worker + DO）

### 7.1 URL

| 用途 | URL | 認證 |
|---|---|---|
| host | `GET {cloud}/v1/relay/host?v=1`（Upgrade: websocket） | `Codync-Sig`，kid = hostSignPub；Worker 由 kid 推導 `computerId`，D1 `computers` 必須存在且 `status='active'`（先 `/v1/host/register`） |
| device | `GET {cloud}/v1/relay/device/{computerId}?v=1[&pair=<offerId>]` | `Codync-Sig`，kid = deviceKey；DO 以 ACL 判斷 |

`v` 不是 `1` → HTTP `426 {"error":{"code":"upgradeRequired"}}`。`{computerId}` 必須符合 `^[A-Za-z0-9_-]{22}$`，否則 `400`（不轉給 DO）。

Worker → DO 的轉送**不沿用 client 的 URL**：Worker 自己建立新 request `new Request("https://do/relay", {headers})`（只帶 `Upgrade`、`Sec-WebSocket-*` 與下列內部 header），交給 `env.RELAY.get(env.RELAY.idFromName(computerId))`。內部 header：`X-Codync-Internal: 1`、`X-Codync-Role`、`X-Codync-Key`、`X-Codync-Nonce`、`X-Codync-Ts`、`X-Codync-Pair`（offerId 或空）、`X-Codync-Computer`。DO 只以 pathname 完全等於 `/relay`、`/internal/block`、`/internal/changed` 路由，且全部要求 `X-Codync-Internal: 1`；其他 → `404`。因為 client 的 request 永遠不會被原樣轉進 DO，外部無法觸及 `/internal/*`。DO 檢查 nonce 重放後才 `acceptWebSocket`。

Device 准入（DO）：
- `dk` 在 `blocked` 表 → `403`。
- `pair` 參數存在：`offerId` 必須在 ACL `offers` 且未過期；同時最多 2 條 pairing socket，每條最長 120 秒、最多 16 則訊息；不能 `mbox.*`。
- 否則 `dk` 必須列在最新 ACL 的 `devices`。**DO 不看 lease**：lease 只由 host 在握手與每個 request 時判斷，所以 host 離線多久，帳號裝置都能連上 DO 看 presence、用 mailbox（host 上線後處理 `mbox.item` 時再驗一次）。
- DO 從未收過 ACL（host 從未連過）→ `404 {"error":{"code":"unknownComputer"}}`。
- 每台 computer 最多 16 條 device socket → 超過 `429`。

### 7.2 Hibernation 與 attachment

- 一律 `ctx.acceptWebSocket(ws, [role])`，tag 為 `"host"` 或 `"device"`。
- `ws.serializeAttachment`：host `{role:"host", kid, ready: boolean}`；device `{role:"device", dk, link, pair: boolean, offer?: string}`。
- **「host 在線」的唯一定義**：存在一條 host socket 且其 attachment `ready == true`。presence、mailbox 接受與否（§7.7）、`data` 轉送全部用這一個定義。
- `ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair('{"t":"ping"}', '{"t":"pong"}'))`：client 必須送**完全相同**的字串 `{"t":"ping"}`。
- 新的 host 連線到來時，舊 host socket 以 `4009 replaced` 關閉；所有 device socket 收到 `presence online:false`（新 host socket 的 `ready` 之後再送 `online:true`）。
- host 端：relay socket 每次重新連線，都丟棄全部 link 狀態（channel、訂閱、進行中的 request）。
- device 端：每次收到 `presence online:true` 都丟棄舊 channel 金鑰、重新送 `hello`。

### 7.3 Host socket 訊息（外層 JSON，DO 可讀）

Host → DO：

| `t` | 欄位 | 行為 |
|---|---|---|
| `acl` | `d`（ACL JSON bytes 的 b64url）, `sig` | DO 以 host kid 驗簽、`computerId` 必須相符 → 否則 `{"t":"error","code":"badAcl","ver":<已存>}`。`ver` ≤ 已存 ver → `{"t":"error","code":"staleVersion","ver":<已存>}`，host 設 `acl_ver = max(本機, ver) + 1` 重新簽發。成功 → 存起來；以 `4003 revoked` 關閉 dk 不再列在 `devices` 的一般 socket；以 `4410 pairingExpired` 關閉其 offer 不再列在 `offers` 的 pairing socket；**解除封鎖只在**新 ACL 以「不同且非 null 的 `grant`」列出被封鎖的 dk 時（即 host 做了一次新的核准；`blocked` 表存 `(dk, grant_id)`）；回 `{"t":"acl.ok","ver":N}` |
| `ready` | — | host 已完成 state 拉取（§4.3）並發佈 ACL。DO 把 host attachment 設 `ready:true`、送 `presence online:true` 給所有 device、對每條 device socket 送 `open`、開始送 mailbox。之後新接受的 device socket 立即送 `open` |
| `data` | `link`, `m`（channel 訊息物件） | host 必須 ready；將 `m` 原樣（`JSON.stringify(m)`）送到該 link 的 device socket |
| `close` | `link`, `code`, `reason` | 關閉該 device socket |
| `mbox.ack` | `seq`, `ok`（bool）, `code`（`ok:false` 時：`unauthorized`/`unknownBot`/`invalid`） | 刪除 mailbox 項目，通知發送裝置 `mbox.delivered` 或 `mbox.failed` |
| `ping` | — | auto-response |

DO → Host：

| `t` | 欄位 |
|---|---|
| `open` | `link`, `dk`, `pair`（bool） |
| `data` | `link`, `m`（device 送來的 channel 訊息物件） |
| `close` | `link`（device 斷線） |
| `mbox.item` | `seq`, `from`（dk）, `nonce`（clientNonce）, `d`（blob b64url）, `exp` |
| `cloud.changed` | —（Worker 通知：請立刻拉 `/v1/host/state`） |
| `acl.ok` / `error` | 見上 |

### 7.4 Device socket 訊息

Device → DO：channel 訊息（`hello`、`f`）原樣送出，DO 包成 `{"t":"data","link","m"}` 轉給 host（host 不在線（§7.2 定義）→ 丟棄，並回一次 `presence`）。以及：

| `t` | 欄位 | 回應 |
|---|---|---|
| `mbox.put` | `nonce`（clientNonce）, `exp`（≤ now+24h；缺省 now+24h）, `d`（blob） | `{"t":"mbox.ok","nonce","state":"queued"}`；錯誤 `{"t":"mbox.err","nonce","code"}`，code：`hostOnline`（請改用 channel）、`tooLarge`、`full`、`invalid`、`pairing` |
| `mbox.cancel` | `nonce` | 未投遞 → 刪除並回 `{"t":"mbox.cancelled","nonce"}`；已送出給 host 但未 ack、或不存在 → `{"t":"mbox.gone","nonce","state":"delivering"|"unknown"}` |
| `mbox.list` | — | `{"t":"mbox.items","items":[{"nonce","exp","state":"queued"|"delivering"}]}`（只列該 dk 的） |
| `ping` | — | auto-response `pong` |

DO → Device（除了 host 轉來的 channel 訊息 `welcome`/`reject`/`f` 之外）：

| `t` | 欄位 |
|---|---|
| `presence` | `online`（bool）, `lastSeenAt`（ms 或 null）— 連線建立時送一次，之後狀態變化時送（含 host 被 replaced 時的 false→true） |
| `mbox.delivered` | `nonce` |
| `mbox.failed` | `nonce`, `code` |
| `mbox.expired` | `nonce` |
| `mbox.*` 回應 | 見上 |

Channel 訊息與 DO 訊息的 `t` 值不重疊：channel 只有 `hello`/`welcome`/`reject`/`f`。

### 7.5 Client 連線策略（Apple 與整合測試共通）

1. 平行對所有 `urls`（`http://h:p` → `ws://h:p/channel?v=1`）開 WebSocket 並握手；**1.5 秒**內第一個完成握手的勝出。
2. 都失敗且有 `cloud` → 連 `{cloud}/v1/relay/device/{computerId}`。每次收到 `presence online:true` 就（重新）送 `hello`；`online:false` → 狀態 `computerOffline(lastSeen)`，保持 socket（可用 mailbox），等下一個 `online:true`。
3. 兩者都不可用 → `offline("Can't reach …")`，退避重試。
4. App 回前景、網路路徑改變（`NWPathMonitor`）→ 重新跑 1–3（可能從中繼換回直連）；events 以當前 `rev` 重新訂閱。
5. 每次 inner `hello` 成功後，把回應的 `urls`、`cloud`、`device`、`name`、`boxKey` 合併進持久化的 `Computer`（§3.3、§10.2）；帳號來源的 computer 因此在第一次中繼連線後也能試 LAN 直連。

### 7.6 心跳、重連、限流

- 心跳：host 與 device 每 30 秒送 `{"t":"ping"}`（中繼）；直連時 host 每 15 秒送 WebSocket Ping frame，client 45 秒沒收到任何資料視為斷線。
- DO alarm：只要有 socket 或 mailbox 項目就每 60 秒一次：host socket 的 `ctx.getWebSocketAutoResponseTimestamp()` 超過 90 秒 → 以 `1011` 關閉並標記離線；清除過期 mailbox（通知 `mbox.expired`）與過期 nonces。
- 重連退避：host 1 s 起倍增至 60 s、device 1 s 起倍增至 30 s，皆 full jitter；穩定 60 秒後重置。
- `4100 paired` → 立即以一般模式重連（§4.1）。
- 不重試的 close code：`4001 unauthorized`、`4003 revoked`、`4410 pairingExpired`（device 顯示需重新授權）；`4009 replaced` → host 等 5 分鐘再試並記錄 `error`（可能有人複製了 `identity.json`）；`4400 unsupportedVersion` → 顯示需要更新。
- 限流（DO 記憶體 token bucket，休眠後重置可接受）：**只計 device → DO 方向**，每條 device socket 每 10 秒 ≤ 200 則、≤ 8 MiB；超過 `4008 rateLimited`。host → device（含 `sync` 補送）不限。
- 單則 WebSocket 訊息 ≤ 1 MiB（Cloudflare 限制）；§6.3 的 256 KiB chunk 保證不超過。

Close code 總表：`1000` 正常、`1011` 內部錯誤／逾時、`4001 unauthorized`、`4002 protocolError`、`4003 revoked`、`4004 unknownComputer`、`4008 rateLimited`、`4009 replaced`、`4011 rekey`、`4013 tooLarge`、`4100 paired`、`4400 unsupportedVersion`、`4410 pairingExpired`。

### 7.7 Mailbox 規則

- 只在 host 不在線時接受（§7.2 的唯一定義：沒有 `ready:true` 的 host socket）。
- 冪等：`(dk, nonce)` 唯一；重送同一組回傳現有狀態，不重複入列。
- 上限：單則 blob ≤ 64 KiB；每 dk ≤ 50 則；每 computer ≤ 500 則、總計 ≤ 8 MiB。
- 投遞：host `ready` 後，DO 依 `seq` 遞增逐則送 `mbox.item`（狀態改 `delivering`）；host 逐則處理並 `mbox.ack`。host 斷線時 `delivering` 退回 `queued`，重連後重送（host 以 clientNonce 去重）。
- TTL 到期 → 刪除 + `mbox.expired`。

---

## 8. CLOUD track：`cloud/`

### 8.1 結構與設定

```
cloud/
  package.json  tsconfig.json  wrangler.toml  README.md
  migrations/0001_init.sql
  src/index.ts          路由（可用 hono，或像 relay/ 一樣手寫）
  src/auth.ts           Clerk 驗證、Codync-Sig 驗證
  src/api.ts            /v1 handlers
  src/relay.ts          ComputerRelay Durable Object
  test/*.test.ts        vitest + @cloudflare/vitest-pool-workers
  test/e2e/relay.e2e.ts 跨 track 整合測試
```
依賴：`@clerk/backend`、`svix`（webhook）；測試：`vitest`、`@cloudflare/vitest-pool-workers`、`@noble/curves`、`@noble/ciphers`、`@noble/hashes`。Worker 端 Ed25519 驗證用 WebCrypto，不引入加密套件。

`wrangler.toml`：
```toml
name = "codync-cloud"
main = "src/index.ts"
compatibility_date = "2026-09-01"
compatibility_flags = ["nodejs_compat"]

[[migrations]]
tag = "v1"
new_sqlite_classes = ["ComputerRelay"]

# production（部署需 owner 確認）
[vars]
CLERK_ISSUER = "<production Clerk Frontend API URL>"
[[d1_databases]]
binding = "DB"
database_name = "codync"
database_id = "<production id>"
migrations_dir = "migrations"
[[durable_objects.bindings]]
name = "RELAY"
class_name = "ComputerRelay"
[triggers]
crons = ["*/15 * * * *"]

[env.staging]
vars = { CLERK_ISSUER = "https://sunny-mollusk-8651.clerk.accounts.dev" }
d1_databases = [{ binding = "DB", database_name = "codync-staging", database_id = "<由 wrangler d1 create 取得>", migrations_dir = "migrations" }]
durable_objects.bindings = [{ name = "RELAY", class_name = "ComputerRelay" }]
triggers = { crons = ["*/15 * * * *"] }

[env.dev]   # wrangler dev / e2e：本機 D1，CLERK_JWT_KEY 由測試注入
vars = { CLERK_ISSUER = "https://clerk.test.local" }
d1_databases = [{ binding = "DB", database_name = "codync-dev", database_id = "dev", migrations_dir = "migrations" }]
durable_objects.bindings = [{ name = "RELAY", class_name = "ComputerRelay" }]
```
Worker 名稱：production `codync-cloud`，staging `codync-cloud-staging`。

### 8.2 D1 schema（`migrations/0001_init.sql`）

```sql
CREATE TABLE accounts (
  user_id     TEXT PRIMARY KEY,                 -- Clerk sub
  email       TEXT,
  status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','deleted')),
  created_at  INTEGER NOT NULL,
  deleted_at  INTEGER
);

CREATE TABLE computers (
  id            TEXT PRIMARY KEY,               -- computerId
  sign_pub      TEXT NOT NULL UNIQUE,
  box_pub       TEXT NOT NULL,
  owner_user_id TEXT REFERENCES accounts(user_id),
  name          TEXT NOT NULL,
  platform      TEXT NOT NULL CHECK (platform IN ('macos','linux')),
  device        TEXT,                           -- laptop|macmini|…|linux（host 的 Device）
  version       TEXT NOT NULL,
  status        TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','blocked')),
  online        INTEGER NOT NULL DEFAULT 0,     -- DO 寫入
  last_seen_at  INTEGER,                        -- DO 寫入
  claimed_at    INTEGER,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);
CREATE INDEX computers_owner ON computers(owner_user_id);

CREATE TABLE devices (
  id            TEXT PRIMARY KEY,               -- dev_…
  owner_user_id TEXT NOT NULL REFERENCES accounts(user_id),
  sign_pub      TEXT NOT NULL,
  name          TEXT NOT NULL,
  platform      TEXT NOT NULL CHECK (platform IN ('ios','macos')),
  created_at    INTEGER NOT NULL,
  last_used_at  INTEGER,
  revoked_at    INTEGER,
  UNIQUE (owner_user_id, sign_pub)
);

CREATE TABLE claims (
  id          TEXT PRIMARY KEY,                 -- clm_…
  user_id     TEXT NOT NULL REFERENCES accounts(user_id),
  nonce       TEXT NOT NULL,
  expires_at  INTEGER NOT NULL,
  consumed_at INTEGER,
  computer_id TEXT
);

CREATE TABLE access_requests (
  id          TEXT PRIMARY KEY,                 -- req_…
  computer_id TEXT NOT NULL REFERENCES computers(id),
  device_id   TEXT NOT NULL REFERENCES devices(id),
  user_id     TEXT NOT NULL REFERENCES accounts(user_id),
  status      TEXT NOT NULL CHECK (status IN ('pending','approved','denied','expired','cancelled')),
  commit_hash  TEXT NOT NULL,                   -- SHA-256("codync/sascommit/v1" ‖ dk ‖ nD)
  host_nonce   TEXT,                            -- nH，host 設一次
  device_nonce TEXT,                            -- nD，host_nonce 存在後裝置才可設
  created_at  INTEGER NOT NULL,
  expires_at  INTEGER NOT NULL,                 -- created_at + 10 min
  decided_at  INTEGER,
  grant_id    TEXT
);
CREATE INDEX access_requests_computer ON access_requests(computer_id, status);
CREATE UNIQUE INDEX access_requests_one_pending ON access_requests(computer_id, device_id) WHERE status = 'pending';

CREATE TABLE grants (
  id             TEXT PRIMARY KEY,              -- grt_…
  computer_id    TEXT NOT NULL REFERENCES computers(id),
  device_id      TEXT NOT NULL REFERENCES devices(id),
  scopes         TEXT NOT NULL,                 -- JSON array
  status         TEXT NOT NULL CHECK (status IN ('active','revoked')),
  created_at     INTEGER NOT NULL,
  revoked_at     INTEGER,
  revoked_reason TEXT                           -- owner|deviceRevoked|unclaimed|accountDeleted
);
CREATE UNIQUE INDEX grants_active ON grants(computer_id, device_id) WHERE status = 'active';

CREATE TABLE sig_nonces (
  kid        TEXT NOT NULL,
  nonce      TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  PRIMARY KEY (kid, nonce)
);

CREATE TABLE processed_events (
  provider     TEXT NOT NULL,
  event_id     TEXT NOT NULL,
  processed_at INTEGER NOT NULL,
  PRIMARY KEY (provider, event_id)
);

CREATE TABLE audit_events (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  actor      TEXT NOT NULL,                     -- user_id / computer:<id> / system
  action     TEXT NOT NULL,
  target     TEXT,
  result     TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX audit_time ON audit_events(created_at);
```

原子性要求（每項以單一 `db.batch([...])` 完成，並檢查 `meta.changes`）：
- **claim complete**：`UPDATE claims SET consumed_at=?t, computer_id=?c WHERE id=?id AND user_id=?u AND consumed_at IS NULL AND expires_at>?now` → `UPDATE computers SET owner_user_id=?u, claimed_at=?t, updated_at=?t WHERE id=?c AND owner_user_id IS NULL AND EXISTS(SELECT 1 FROM claims WHERE id=?id AND consumed_at=?t)`。第一句 0 changes → `410 claimExpired`；第二句 0 changes 且 owner 已是同一 user → 200（冪等）；其他 owner → `409 alreadyClaimed`。
- **approve**：`UPDATE access_requests SET status='approved', decided_at=?, grant_id=?g WHERE id=? AND computer_id=? AND status='pending' AND expires_at>?now` → `INSERT INTO grants (...) SELECT ... WHERE EXISTS(SELECT 1 FROM access_requests WHERE id=? AND grant_id=?g) ON CONFLICT DO NOTHING`；若因 `grants_active` 衝突，回傳既有 active grant。
- **reveal**：`UPDATE access_requests SET device_nonce=? WHERE id=? AND user_id=? AND status='pending' AND host_nonce IS NOT NULL AND device_nonce IS NULL`；0 changes → 依狀態回 `409 conflict`／`410 requestExpired`／`404`。Worker **不**驗 commit（host 驗）。
- **host nonce**：`UPDATE access_requests SET host_nonce=? WHERE id=? AND computer_id=? AND status='pending' AND host_nonce IS NULL`；0 changes → `409 conflict`。
- 所有查詢都帶 owner 範圍；別人的資源一律回 `404 notFound`（不洩漏存在）。建立 access request 同時要求 `computers.owner_user_id = clerk.sub` **且** `devices.owner_user_id = clerk.sub`（device 由 Sig(dev) 的 kid 找出），任一不符 → `404`。

Cron（每 15 分鐘）：刪除 `sig_nonces` 過期、`claims` 過期 1 天、`access_requests` pending 過期 → `expired`（同時清空 `host_nonce`／`device_nonce`）、`audit_events` 超過 30 天。

### 8.3 認證

- **Clerk**：`Authorization: Bearer <session JWT>`。以 `@clerk/backend` 的 `verifyToken(token, opts)` 驗證：有 `CLERK_JWT_KEY` 時用 `jwtKey`（networkless，dev/e2e 使用），否則用 `secretKey: CLERK_SECRET_KEY`（自動抓 JWKS、快取、支援輪替）；`authorizedParties` 取 `CLERK_AUTHORIZED_PARTIES`（逗號分隔，可空；native token 通常沒有 `azp`，有則必須在清單內）。另外檢查 `iss === CLERK_ISSUER`、`sub` 存在、`sid` 存在。`email` 取自自訂 session claim `email`（owner 在 Clerk 設定，§14.3），缺則為 null。每個 Clerk 請求 `INSERT OR IGNORE INTO accounts`；`status='deleted'` → `403 accountDeleted`。
- **Codync-Sig**：§5。
- **Webhook**：`POST /v1/webhooks/clerk`，`svix` 以 `CLERK_WEBHOOK_SECRET` 驗證，`svix-id` 寫入 `processed_events` 去重。

錯誤格式（所有 `/v1`）：
```json
{"error":{"code":"notFound","message":"…"},"requestId":"<cf-ray 或 uuid>"}
```
code 清單：`badRequest`(400) `unauthenticated`(401) `badSignature`(401) `forbidden`(403) `accountDeleted`(403) `notFound`(404) `alreadyClaimed`(409) `conflict`(409) `claimExpired`(410) `requestExpired`(410) `upgradeRequired`(426) `rateLimited`(429) `internal`(500)。

### 8.4 `/v1` HTTP API

所有 body 為 JSON；時間為 ms。「Clerk」= 需要 Bearer；「Sig(dev)」= 需要該 device key 的 `Codync-Sig`；「Sig(host)」= host key 的 `Codync-Sig`。

**公開**
- `GET /v1/health` → `{"ok":true,"version":"3.0.0"}`

**Clerk（使用者）**
- `GET /v1/me` → `{"userId","email","createdAt"}`
- `POST /v1/devices`（Clerk + Sig(dev)）`{"name","platform":"ios"|"macos"}` → `{"device":{"deviceId","deviceKey","name","platform","createdAt"}}`。以 `(owner, sign_pub)` upsert；已撤銷的同一把 key → `403 forbidden`（需換 key）。
- `GET /v1/devices` → `{"devices":[{"deviceId","deviceKey","name","platform","createdAt","lastUsedAt","revoked":bool}]}`
- `DELETE /v1/devices/{deviceId}` → `{}`；撤銷裝置與其所有 grants（`deviceRevoked`），對相關 computers 呼叫 DO `/internal/block` + `cloud.changed`。
- `POST /v1/claims` → `{"claimId","nonce","expiresAt"}`（nonce = 16 random bytes b64url）
- `POST /v1/claims/{claimId}/complete` `{"computerId","signKey","boxKey","name","platform","device","version","sig"}` → `{"computer":<Computer>}`。驗：`computerId == derive(signKey)`、sig（§4.2 A，輸入含 `boxKey`）、claim 有效；computer 列不存在時一併建立，`box_pub` 一律取自簽過的 `boxKey`。
- `GET /v1/computers`（Clerk，選配 Sig(dev)）→ `{"computers":[Computer]}`，
  `Computer = {"computerId","name","platform","device","signKey","boxKey","version","online":bool,"lastSeenAt","claimedAt","access":"none"|"pending"|"granted"|null}`（`access` 只在帶 Sig(dev) 且該 device 屬於此帳號時計算；`boxKey` 僅供顯示，client 不得用於封裝，§3.3）
- `PATCH /v1/computers/{id}` `{"name"}` → `{"computer":…}`（只改雲端顯示名稱）
- `DELETE /v1/computers/{id}` → `{}`；解除綁定：owner=NULL、grants 全撤（`unclaimed`）、pending requests → `cancelled`、DO block + changed。
- `POST /v1/computers/{id}/access-requests`（Clerk + Sig(dev)，device 須已註冊未撤銷；computer 與 device 都必須屬於 `clerk.sub`，否則 `404`）`{"commit"}` → `{"requestId","expiresAt"}`；已有 pending → 先把它改為 `cancelled` 再建新的（commit 不可重用）；已有 active grant → `409 conflict`。成功後 DO `cloud.changed`。每帳號每小時最多 20 筆 → `429`。
- `GET /v1/access-requests/{id}` → `{"requestId","computerId","status","expiresAt","hostNonce"?,"grantId"?}`（只有申請者可見）
- `POST /v1/access-requests/{id}/reveal`（Clerk + Sig(dev)，必須是申請的那個 device）`{"nonce"}` → `{}`；成功後 DO `cloud.changed`。
- `DELETE /v1/access-requests/{id}` → `{}`（申請者取消 pending）
- `GET /v1/computers/{id}/grants` → `{"grants":[{"grantId","deviceId","deviceName","platform","scopes","createdAt"}]}`
- `DELETE /v1/computers/{id}/grants/{grantId}` → `{}`（`owner`）+ DO block + changed

**Sig(host)（不需要帳號）**
- `POST /v1/host/register` `{"boxKey","name","platform","device","version"}` → `{"computerId","owned":bool}`。以 kid 推導 id；upsert（`name`/`device`/`version`/`box_pub`/`updated_at`）。同 IP 每分鐘最多 10 次新建（Workers Rate Limiting binding；平台不支援時先略過並在 README 註記）。
- `GET /v1/host/state` →
  ```json
  {"owner":{"userId":"user_…","email":"a@b.c"} | null,
   "grants":[{"grantId","deviceKey","deviceName","platform","scopes":["control","screen"]}],
   "requests":[{"requestId","deviceKey","deviceName","platform","email","commit","hostNonce"?,"deviceNonce"?,"createdAt","expiresAt"}]}
  ```
  grants 只含：grant active、device 未撤銷、device.owner == computer.owner。host 只把它當成「可續約／應刪除」的清單（§4.3）。
- `POST /v1/host/access-requests/{id}/nonce` `{"nonce"}` → `{}`（§8.2；已設過 → `409`）
- `POST /v1/host/grants/{grantId}/revoke` `{}` → `{}`（host 在本機撤銷帳號來源裝置時同步 D1，`revoked_reason='owner'`；失敗只記 `warn`，本機撤銷已生效）
- `POST /v1/host/access-requests/{id}/decision` `{"decision":"approve"|"deny"}` → `{"status","grantId"?}`（§8.2 原子性；request 過期 → `410 requestExpired`；approve 要求 `device_nonce IS NOT NULL`，否則 `409`）
- `POST /v1/host/unclaim` `{}` → `{}`（在電腦上移出帳號；同 `DELETE /v1/computers/{id}` 的效果）

**Relay**：§7.1。**Webhook**：`user.deleted` → accounts `status='deleted'`、devices 撤銷、grants 撤銷（`accountDeleted`）、computers owner=NULL、DO block + changed。

### 8.5 `ComputerRelay` DO

- DO SQLite 表：`meta(k TEXT PRIMARY KEY, v TEXT)`（`signKey`、`aclVer`、`aclD`、`aclSig`、`lastSeenAt`）、`mailbox(seq INTEGER PRIMARY KEY AUTOINCREMENT, dk TEXT, nonce TEXT, blob BLOB, size INTEGER, exp INTEGER, state TEXT, UNIQUE(dk, nonce))`、`nonces(kid TEXT, nonce TEXT, exp INTEGER, PRIMARY KEY(kid, nonce))`、`blocked(dk TEXT PRIMARY KEY, grant_id TEXT, blocked_at INTEGER)`。
- 內部端點（只經 Worker 自建的 request 呼叫，§7.1）：`POST /internal/block {"devices":[{"dk","grantId"}]}`（upsert `blocked`、以 `4003` 關閉相符 socket、送 host `cloud.changed`）、`POST /internal/changed`（送 host `cloud.changed`；host 離線則忽略）。解除封鎖規則見 §7.3 `acl`。
- host 連上／斷線 → `UPDATE computers SET online=?, last_seen_at=? WHERE id=?`（D1 binding；列不存在即 no-op）。
- DO 不解析 `f` 的內容、不保存 channel 訊息。

### 8.6 CLOUD 測試

- 單元（vitest-pool-workers）：Codync-Sig 向量（含 authority）、重放、時間窗、跨 authority 拒絕；claim 向量；ACL 驗證（向量 `acl`）、ver 單調與 `staleVersion` 回傳已存 ver；封鎖後以相同 grant 的 ACL 不解封、以新 grant 解封；`computerId` 格式錯誤 400、偽造 `/internal/*` 路徑不可達；access request 跨帳號 computer 404、reveal 早於 host nonce 409、approve 早於 reveal 409；host 未 ready 時 mailbox 仍接受、`data` 丟棄；ready 後新 device socket 立即收到 `open`；host replaced 時 device 收到 false→true presence；claim 競爭（兩帳號同時 complete）；approve 雙擊；跨帳號 404；webhook 去重；mailbox 冪等、上限、取消、TTL、依序投遞、host 斷線重送；presence；auto-response；device 准入（封鎖、pairing 限制、offer 消失 → 4410）；host 離線超過 15 分鐘帳號裝置仍可 `mbox.put`；`relay/` 的 `mutableContent` 透傳。
- 以 `@noble/*` 在 TS 寫一份 device／host 參考實作（測試用），先通過 `remote-relay-vectors.json`，再用於 e2e。

---

## 9. HOST track：`host/src`

### 9.1 新模組

| 檔案 | 內容 |
|---|---|
| `identity.rs` | `Identity { sign: SigningKey, box_secret: StaticSecret }`、`load_or_create(dir)`、`computer_id()`、`sign_pub_b64()`、`box_pub_b64()`、`sign_request(method, path_query, body) -> String`（Codync-Sig header 值） |
| `crypto.rs` | 握手、frame seal/open、chunking、mailbox open、SAS commit／code、offer id、claim 簽章輸入、push seal；全部以向量測試 |
| `channel.rs` | 與傳輸無關的 channel：`run(hub, Transport, ChannelKind)`；握手、授權、inner RPC、訂閱、lease 監看 |
| `relay.rs` | 對 `{cloud}/v1/relay/host` 的出站 WebSocket：ACL 發佈、link 多工、mailbox 處理、重連 |
| `cloud.rs` | `/v1/host/*` HTTP client（`crate::http()`）、state 輪詢、核准決定、`CloudStatus` |
| `devices.rs`（或併入 `store.rs`） | 授權裝置表、`Caller`、`Scope`、`DeviceSource` |

依賴（最新相容版本）：`ed25519-dalek`（`rand_core`）、`x25519-dalek`（`static_secrets`）、`chacha20poly1305`、`hkdf`、`rand_core`（`getrandom`）、`tokio-tungstenite`（rustls + webpki roots）、`axum` 開 `ws` feature。注意 `hkdf` 與既有 `sha2 = 0.11` 的 `digest` 版本需一致；必要時 `hkdf` 用自己相容的 `sha2`，不要手寫 HKDF。

### 9.2 Store

`Store::open` 新增 schema 版本（kv `schema`）；升到 3 時 `DROP TABLE devices`（舊 push ticket 表，手機重連時會重新註冊）並建立：
```sql
CREATE TABLE devices(
  key TEXT PRIMARY KEY, name TEXT NOT NULL, platform TEXT NOT NULL,
  source TEXT NOT NULL, grant_id TEXT, scopes TEXT NOT NULL,
  lease_until INTEGER, created_at INTEGER NOT NULL, last_seen_at INTEGER);
CREATE TABLE push_tickets(
  ticket TEXT PRIMARY KEY, device_key TEXT NOT NULL, push_key TEXT, ctx TEXT, name TEXT, created_at INTEGER NOT NULL);
CREATE TABLE activity_tickets(
  ticket TEXT PRIMARY KEY, bot_id TEXT NOT NULL, device_key TEXT NOT NULL, created_at INTEGER NOT NULL);
```
`hub.activities`（記憶體 map）改由 `activity_tickets` 取代。
Rust 以 enum 表示：`DeviceSource { Local, Account }`、`Scope { Control, Screen }`（serde camelCase 字串）。刪除裝置時同一 transaction 刪除其 `push_tickets` 與 `activity_tickets`。kv 新增：`acl_ver`、`cloud_url`、`cloud_enabled`。

`acl_ver` 與 DO 的已存 ver 可能不一致（資料庫被刪、還原，但 `identity.json` 還在）：收到 `staleVersion`／`badAcl` 附帶的 `ver` 時，設 `acl_ver = max(acl_ver, ver) + 1` 並重新簽發一次（只重試一次，再失敗記 `error`）。測試：刪除資料庫、保留 identity 後重啟，ACL 能被 DO 接受。

### 9.3 Hub

新增欄位：`identity: Identity`、`auth: watch::Sender<u64>`（裝置表 generation；每次變動 +1，channel 據此重新檢查自己的 dk）、`pairing_codes: Mutex<Vec<PairingCode>>`、`cloud: Mutex<CloudStatus>`、`access_requests: Mutex<Vec<AccessRequest>>`（含 host 自己的 `nH`，只存記憶體；host 重啟後對同一申請 post nonce 會得到 409 → 該申請自動 deny，手機重新申請）。刪除 `activities`（改存 `activity_tickets`，§9.2）。`token` 保留（loopback）。

### 9.4 API 變更（`api.rs`）

- `authorized()`：只接受 `Authorization: Bearer`（刪除 `?token=`）**且** `peer.ip().to_canonical().is_loopback()`（`--bind ::` 時 IPv4 會以 `::ffff:127.0.0.1` 出現；附單元測試）。`events`／`term_stream`／`statusline` 都要 `ConnectInfo`。
- `dispatch(hub, caller: &Caller, method, body)`；`enum Caller { Local, Device { key: String, scopes: Vec<Scope> }, Pairing { key: String } }`；權限依 §6.6，拒絕回 403。取代今日只針對 `setScreenEnabled` 的特例。
- 抽出 `events_stream(hub, since, client, caller) -> impl Stream<Item = Value>`（SSE 與 channel 共用；`Caller::Device` 看不到 `accessRequests`／`cloud` 事件）與 `term_events(hub, id)`。
- `GET /health` → `{"ok":true,"hostId","computerId","version"}`。
- `GET /channel?v=1`：axum WebSocket，交給 `channel::run`。
- `hello` 回應新增：`computerId`、`signKey`、`boxKey`、`protocol: 1`、`cloud`（啟用時的 cloud URL，否則 null）。events 的 `hello` 事件新增 `computerId`。
- `registerDevice {ticket, name, relay, pushKey, ctx}`：ticket 綁定 `Caller::Device.key`（Local 呼叫則 `device_key = "local"`）；`push::notify` 讀 `push_tickets` 並依 §6.7 封裝（沒有 `pushKey` 的 ticket 只收通用文字）。
- `registerActivity`：ticket 寫入 `activity_tickets`，綁定 caller 的 device key；`live_activity_update` 不再送 `activity` 文字。
- 新 loopback 方法：

| 方法 | body | 回應 |
|---|---|---|
| `pairing` | — | `{"pairingUrl","urls","svg","expiresAt"}`（每次呼叫產生新 code） |
| `devices` | — | `{"devices":[{"key","name","platform","source","scopes","leaseUntil","createdAt","lastSeenAt","connected":bool}]}` |
| `revokeDevice` | `{"key"}` | `{}` |
| `accessRequests` | — | `{"requests":[AccessRequest]}` |
| `decideAccessRequest` | `{"requestId","approve":bool}` | `{}`（approve：`nD` 未驗證 → 409；成功後以 host 顯示的 dk + 回傳的 grantId 新增裝置並發佈 ACL） |
| `claimSign` | `{"claimId","nonce","userId"}` | `{"computerId","signKey","boxKey","name","platform","device","version","sig"}` |
| `unclaim` | — | `{}` |
| `cloudStatus` | — | `CloudStatus` |
| `setCloud` | `{"enabled":bool,"url"?}` | `CloudStatus` |

`AccessRequest = {"requestId","deviceKey","deviceName","platform","email","code"(SAS；`nD` 驗證前為 null),"createdAt","expiresAt"}`
`CloudStatus = {"enabled","url","registered","connected","owner":{"userId","email"}|null,"lastError"}`
新 event（只給 Local）：`{"type":"accessRequests","requests":[…]}`、`{"type":"cloud","cloud":CloudStatus}`；不帶 rev（與 `usage`／`screen` 同類）。

### 9.5 Channel（`channel.rs`）

- 同一實作服務兩種傳輸：直連（axum WebSocket）與中繼 link（`relay.rs` 以 mpsc 餵入 `m`、以共用 sink 送出 `{"t":"data","link","m"}`）。
- 流程：等 `hello` → 驗證 → `welcome` → 迴圈：組裝 frame → 解析 inner → request 以 `tokio::spawn` 呼叫 `api::dispatch`（同一 channel 的回應可亂序）→ 回 `ok`／`err`；`sub` 啟動串流任務；`cancel` 中止串流。
- 每個 request 前檢查 dk 仍有效（查 `hub.auth` generation 快取）；`watch` 變動 → 立即重查，失效 → 送 `reject`（或 close `4003`）並結束所有串流。`lease_until` 到期時以 timer 主動關閉。
- 第一個成功解密的 frame 之後才標記 connected、更新 `devices.last_seen_at`（§6.2）。
- pairing channel（`Caller::Pairing`）：只處理一個 `pair`，回應後以 `4100` 關閉（§4.1）。

### 9.6 Relay 與 cloud client

- cloud URL：env `CODYNC_CLOUD_URL` ＞ kv `cloud_url` ＞ `cloud::DEFAULT_CLOUD_URL: Option<&str>`（目前 `None`，production 部署時填入）。三者皆無 → 雲端停用（不重試、`CloudStatus.enabled=false`）。`CODYNC_CLOUD=off` 一律停用；`host/tests/e2e.rs` 設 `CODYNC_CLOUD=off`。
- 啟動（雲端啟用時）：`POST /v1/host/register`（退避重試，最長 30 分鐘間隔）→ 連 `/v1/relay/host` → **state 拉取（§4.3，最多 10 秒）** → 送 `acl` → 等 `acl.ok`（`staleVersion` 處理見 §9.2）→ 送 `ready`。每次重連都走同一順序，並丟棄全部 link 狀態。
- state 拉取：每次 relay 連上時、收到 `cloud.changed` 時、有 owner 時每 5 分鐘；套用 §4.3 規則（只續約／刪除，不新增）；對 `hostNonce` 為空的新申請 post nonce（§4.2 B 4）；`deviceNonce` 出現時驗 commit、算 SAS、emit `accessRequests`；owner 變更時 emit `cloud` 事件。
- `mbox.item`：依序處理；§6.4 驗證；回 `mbox.ack`（`ok`/`code`）。
- ping 每 30 秒；§7.6 退避與 close code 處理。
- tracing 結構化欄位（`computer_id`、`link`、`error = format!("{e:#}")`）；**不記錄**金鑰、密文、pairing code、訊息內容。

### 9.7 CLI

- `codync-host pair [--json]`：改呼叫本機 loopback `pairing`（host 需在執行中，否則明確提示）。JSON：`{"name","computerId","signKey","boxKey","urls","cloud","pairingUrl","expiresAt"}`。
- `codync-host info --json`（不需 host 執行中）：`{"name","computerId","signKey","boxKey","version","port","token","running":bool}` — SSH 設定用；`token` 為 loopback token。`info` **只讀** `identity.json` 與 token，絕不建立：檔案不存在 → stderr `start codync-host once first`、exit 1。只有 `serve` 會建立 identity。
- `codync-host access list|approve <requestId|SAS>|deny <requestId>`、`codync-host devices list|revoke <key>`、`codync-host cloud [--enable|--disable] [--url URL]`：皆走 loopback API。
- TUI `--url`：說明改為「遠端請用 SSH tunnel 轉到 loopback」。

### 9.8 HOST 測試

- `crypto.rs`：通過 `remote-relay-vectors.json` 全部欄位（`include_str!("../../docs/remote-relay-vectors.json")`）。
- 授權：未知 dk 被拒、pairing 流程（回應後 4100、pairing channel 上非 `pair` 被關）、無 code 時 `pair:true` 在 ECDH 前被拒、code 單次、lease 到期關閉 channel、撤權關閉進行中的 events 訂閱並刪 push／activity tickets、scope 拒絕、loopback-only 方法經 channel 被拒、非 loopback bearer 被拒、`::ffff:127.0.0.1` 視為 loopback。
- state 套用：未知 grant 被忽略（表不變）、已知 grant 續約、缺席者刪除；relay 連上後 state 拉取前帳號裝置握手被拒；relay link 上 `hello.dk != open.dk` 被關；SAS commit 不符自動 deny、reveal 前 approve 被拒。
- `acl_ver`：刪 DB 保留 identity → `staleVersion` 後以 `max+1` 成功。
- `tests/e2e.rs` 延伸：以 Rust 測試端的 device 實作走直連 `/channel`：pair → hello → events → `send`（fake agent）→ 收到回覆。

---

## 10. APPLE-KIT track：`kit/`

### 10.1 公開 API（APPLE-APPS 依賴這些名稱與簽章）

```swift
// CodyncKit/Models/Computer.swift
public typealias ComputerID = String
public struct Computer: Codable, Hashable, Sendable, Identifiable {
    public var id: ComputerID
    public var name: String
    public var signKey: String          // pin：QR `sk`，或帳號流程 SAS 通過後的 signKey
    public var boxKey: String?          // pin：只來自 QR `bk` 或 E2E `hello`（§3.3）；nil = 尚無 mailbox
    public var urls: [String]
    public var cloud: URL?
    public var device: String?
    public var color: String?
}
public struct BotReference: Codable, Hashable, Sendable {
    public var accountId: String?       // = SharedStore.Context.accountID（原始 Clerk userID；nil = 本機 context）
    public var computerId: ComputerID
    public var botId: String
}

// CodyncKit/Client/Pairing.swift（取代 HostClient.swift 裡的舊 Pairing）
public struct Pairing: Hashable, Sendable {      // 掃到的 QR；暫存，不持久化
    public var computer: Computer
    public var code: String
    public init?(url: URL); public init?(string: String)
}

// CodyncKit/Client/DeviceIdentity.swift
public struct DeviceIdentity: Sendable {
    public let publicKey: String                  // Ed25519，b64url
    public let pushKey: String                    // X25519，b64url（§6.7）
    public static func load(context: SharedStore.Context) throws -> DeviceIdentity  // 沒有就建立（兩把）
    public static func delete(context: SharedStore.Context)
    public func signatureHeader(method: String, authority: String, pathAndQuery: String, body: Data) throws -> String
    /// NSE 用：以 context.id 找 push key 解開 §6.7 的 `sealed`；失敗回 nil。
    public static func openPush(sealed: String, computerId: ComputerID, contextID: String) -> (title: String, body: String)?
}

// CodyncKit/Client/HostTransport.swift
public enum HostRoute: Sendable, Equatable { case loopback, direct, relay }
public enum HostStreamRequest: Sendable { case events(since: Int64, client: String), term(String) }
public enum LinkState: Sendable, Equatable {
    case connecting, ready(HostRoute), hostOffline(lastSeen: Date?), unauthorized(String), failed(String)
}
public protocol HostTransport: Sendable {
    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data
    /// nonisolated：actor 實作在內部起 Task。每個元素 = 一則 SSE data JSON。
    func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error>
    /// 每次呼叫回傳一條新的 stream：先 yield 目前狀態，之後每次變化。可多個消費者。
    func states() -> AsyncStream<LinkState>
}
// 路由切換（中繼↔直連）或重新握手時，transport 以 HostError.unreachable 結束所有已開的 stream；
// 重新訂閱由 BotStore 負責（以它目前的 rev）。LoopbackTransport.states() 只 yield `.ready(.loopback)`。

public struct HostClient: Sendable {
    public let transport: any HostTransport
    public init(transport: any HostTransport)
    public init(baseURL: URL, token: String)      // loopback（Mac 本機、SSH tunnel）
    // 既有 typed 方法（hello/sync/send/… 與 Market.swift 的擴充）保持原樣，改走 transport
}

public enum HostError: LocalizedError, Sendable {
    case http(Int, String)
    case unreachable
    case computerOffline(lastSeen: Date?)
    case unauthorized(String)
    case upgradeRequired
}

// CodyncKit/Client/HostConnector.swift
public enum HostConnector {
    /// §7.5：直連優先（1.5 s），否則中繼。回傳的 transport 在 host 離線時仍存活（presence／mailbox）。
    public static func connect(_ computer: Computer, identity: DeviceIdentity) async throws -> ChannelTransport
    /// §4.1：以 QR 配對（收到 4100 即完成），回傳可持久化的 Computer。
    public static func pair(_ pairing: Pairing, identity: DeviceIdentity, deviceName: String, platform: String) async throws -> Computer
}
public actor ChannelTransport: HostTransport {
    public nonisolated func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data
    public nonisolated func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error>
    public nonisolated func states() -> AsyncStream<LinkState>
    public nonisolated func computerUpdates() -> AsyncStream<Computer>        // 每次 hello 成功後合併的新值（§7.5 步驟 5）
    // mailbox
    public func enqueue(botId: String, text: String, clientNonce: String) async throws   // 無 boxKey → HostError.computerOffline
    public func cancelQueued(clientNonce: String) async -> MailboxCancel                  // .cancelled / .delivering / .unknown
    public func listQueued() async -> [QueuedItem]
    public nonisolated func mailboxEvents() -> AsyncStream<MailboxEvent>                  // .delivered/.failed(code)/.expired(nonce)
}

// CodyncKit/Client/CloudClient.swift
public struct CloudClient: Sendable {
    public init(baseURL: URL, identity: DeviceIdentity?, token: @escaping @Sendable () async throws -> String)
    public func me() async throws -> CloudAccount
    public func registerDevice(name: String, platform: String) async throws
    public func computers() async throws -> [CloudComputer]
    public func renameComputer(_ id: ComputerID, name: String) async throws
    public func removeComputer(_ id: ComputerID) async throws
    /// §4.2 B 3–5 全流程：commit → 等 hostNonce → reveal；回傳時 `code` 已可顯示。nD 只在記憶體。
    public func requestAccess(_ id: ComputerID, signKey: String) async throws -> AccessTicket  // requestId, expiresAt, code(SAS)
    public func accessStatus(_ requestId: String) async throws -> AccessStatus    // pending/approved/denied/expired/cancelled
    public func cancelAccess(_ requestId: String) async throws
    public func devices() async throws -> [CloudDevice]
    public func revokeDevice(_ deviceId: String) async throws
    public func grants(_ id: ComputerID) async throws -> [CloudGrant]
    public func revokeGrant(_ id: ComputerID, grantId: String) async throws
    public func createClaim() async throws -> ClaimChallenge
    public func completeClaim(_ claimId: String, signed: ClaimSignature) async throws -> CloudComputer
}
public extension HostClient {                  // loopback-only 方法（Mac）
    func claimSign(claimId: String, nonce: String, userId: String) async throws -> ClaimSignature
    func accessRequests() async throws -> [AccessRequest]
    func decideAccessRequest(_ id: String, approve: Bool) async throws
    func devices() async throws -> [AuthorizedDevice]
    func revokeDevice(_ key: String) async throws
    func cloudStatus() async throws -> CloudStatus
    func setCloud(enabled: Bool) async throws -> CloudStatus
    func unclaim() async throws
}

// CodyncKit/Client/SharedStore.swift（取代既有的 pairing/computers/preferredURL/orderedPairing/usage/bots）
public enum SharedStore {
    public static let appGroup: String
    public static let relayURL: String                        // APNs 推播 relay（不變）
    public static var activeAccountID: String? { get set }
    public static var activeContext: Context { get }
    public struct Context {
        public let accountID: String?
        public var id: String { get }                          // digest(accountID) 或 "local"
        public var computers: [Computer] { get nonmutating set }
        public var bots: [BotSnapshot] { get nonmutating set } // BotSnapshot { computerId, bot }
        public var usage: [ComputerID: Usage] { get nonmutating set }
        public var lastComputerId: ComputerID? { get nonmutating set }  // 最近活躍的 BotStore；Usage widget 只看這台
        public func botURL(_ ref: BotReference) -> URL
        public func reference(from url: URL) -> BotReference?  // scope 不符或格式錯 → nil（取代 acceptsBotURL）
    }
    // 移除所有 static 的 pairing / computers / orderedPairing / preferredURL / usage / bots
}
// deep link 格式：codync://bot/<botId>?scope=<Context.id>&computer=<computerId>

// CodyncUI/Store/BotStore.swift（調整）
@MainActor @Observable public final class BotStore {
    public enum Connection: Equatable {
        case unpaired, connecting, online
        case computerOffline(lastSeen: Date?)             // 中繼說電腦不在線
        case offline(String)                              // 完全連不到（無直連也無中繼）
        case unauthorized(String)                         // 撤權／lease 到期／身分不符
    }
    // `isOffline` 對後三者為 true；`== .online` 的既有比較照常可用。
    public enum Route: Sendable, Hashable { case loopback(baseURL: URL, token: String), channel }
    /// `.channel` 時 BotStore 自己以 `DeviceIdentity.load(context: storage)` 取得身分。
    public init(computer: Computer, route: Route, clientKind: String, storage: SharedStore.Context)
    public private(set) var computer: Computer
    public private(set) var client: HostClient?
    public private(set) var connection: Connection
    public private(set) var hostRoute: HostRoute?
    public private(set) var accessRequests: [AccessRequest]  // 只有 loopback 會有
    public private(set) var cloud: CloudStatus?              // 只有 loopback 會有
    public var onConnected: (@MainActor (BotStore) -> Void)? // 取代 onPaired
    public var onUsageChanged: (@MainActor (ComputerID, Usage) -> Void)?
    public func cancelQueued(_ entry: Entry)
    // 移除：pair(_:)、forget(_:)、unpair()、computers、pairing、persistsPairing
}

// CodyncUI/Store/AccountStore.swift（新增）
@MainActor @Observable public final class AccountStore {
    public init(storage: SharedStore.Context, clientKind: String, cloud: CloudClient?)
    public let storage: SharedStore.Context
    public var accountId: String? { storage.accountID }
    public private(set) var computers: [Computer]
    public private(set) var stores: [ComputerID: BotStore]
    public private(set) var cloudComputers: [CloudComputer]      // 帳號中但本機尚未授權者也列出
    public private(set) var pendingAccess: [ComputerID: AccessTicket]
    public var selection: BotReference?
    public var roster: [RosterItem] { get }                       // 跨 computer 彙整、排序同 BotStore.roster
    public func store(for id: ComputerID) -> BotStore?
    public func pair(_ pairing: Pairing, deviceName: String, platform: String) async throws -> Computer
    public func attach(_ computer: Computer, route: BotStore.Route) // Mac 本機／SSH
    public func detach(_ id: ComputerID)
    public func forget(_ id: ComputerID)
    public func refreshCloud() async
    public func requestAccess(_ id: ComputerID) async throws -> AccessTicket
    public func setColor(_ id: ComputerID, _ color: String)
    public func setActive(_ active: Bool)
    public func retire()
    public var onBotUpdated: (@MainActor (BotReference, Bot) -> Void)?
    public var onConnected: (@MainActor (BotStore) -> Void)?
    public var onSent: (@MainActor (BotReference, Bot) -> Void)?
}
public struct RosterItem: Identifiable, Sendable { public var ref: BotReference; public var bot: Bot; public var computer: Computer; public var id: BotReference { ref } }
```
型別欄位（`CloudComputer`、`AccessTicket`、`AccessStatus`、`AccessRequest`、`AuthorizedDevice`、`CloudStatus`、`ClaimChallenge`、`ClaimSignature`、`CloudAccount`、`CloudDevice`、`CloudGrant`、`QueuedItem`、`MailboxEvent`、`MailboxCancel`、`BotSnapshot`）直接對應 §7.4／§8.4／§9.4 的 JSON，`Codable` 解碼寬鬆（未知欄位忽略、選填欄位 optional）。以上簽章是最小集合；kit 可以多加，不能改名或改型別。

### 10.2 實作要點

- `LoopbackTransport`：今日 `HostClient.call`／`sse` 的邏輯搬過來（Bearer header，無 `?token=`）。
- `ChannelTransport`（actor）：WebSocket（`URLSessionWebSocketTask`）＋ §6 加密＋ §6.5 RPC 多工；`states()`、mailbox API 如上。收到 `4100` 時自動以一般模式重連。
- `HostConnector.connect(computer:identity:)` 實作 §7.5；`HostConnector.pair(_:identity:deviceName:platform:)` 實作 §4.1。
- `BotStore.runStream`：訂閱 `transport.states()` 推 `connection`；stream 以 `unreachable` 結束時以目前 `rev` 重新訂閱；`computerOffline` 時不做盲目退避，等 presence。`computerUpdates()` 的新值寫回 `computer` 並由 AccountStore 持久化。`send` 在 `computerOffline` 且有 `boxKey` 時：本地 entry `status = "waiting"`，成功入列後保持，`mbox.delivered` 後等 events 的正式 entry 取代（既有 clientNonce 取代邏輯），`failed`/`expired` → `"failed"`（可重試：重試 = 重新封裝、新的 `epk`，§6.4）。`cancelQueued` → 成功則移除本地 entry；`delivering` 顯示「已送出」。`status == "waiting"` 的本地 entry **要寫入快取**（重開 App 仍看得到並可取消），啟動時以 `listQueued()` 對帳。
- 快取檔名改以 `computer.id` 取代 `pairing.token` 的 digest。`SharedStore.Context.computers` 不含任何秘密，可留在 App Group UserDefaults。舊資料不遷移，使用者重新配對。
- `onUsageChanged` 寫 `storage.usage[computerId]`；BotStore 變成 active／送出訊息時寫 `storage.lastComputerId`。
- `BotActivityAttributes` 新增 `computerId`；`ContentState.activity` 保留欄位但 host 永遠送空字串（§6.7）。
- `ThreadView`、`BotRow`、`ChatRows`：顯示 `computerOffline`（「電腦目前離線，訊息會在它上線時送出」）、`unauthorized`、`waiting` 狀態（含取消按鈕，icon-only + accessibility label）。
- kit 不依賴 ClerkKit；Clerk token 由 App 以 closure 注入 `CloudClient`。
- Keychain：§3.2（macOS 不設 data protection keychain）。

### 10.3 APPLE-KIT 測試（`swift test`）

`remote-relay-vectors.json` 全欄位（握手、frame、chunk、mailbox、SAS commit／code、offer、Codync-Sig、push）；mailbox 連續封裝兩次 `epk` 不同；`Pairing(url:)` 的 v3 解析與拒絕（`id` 不符、缺 `code`、`cloud` 非 https）；`botURL`／`reference(from:)` 來回與跨 scope 拒絕；`states()` 多消費者；RPC 多工與取消（以假的 WebSocket）；`BotStore` 在 `computerOffline` 時的 `waiting`／取消／到期；`AccountStore` 兩台 computer 相同 bot ID 不混淆、帳號切換後舊回覆不寫入。

---

## 11. APPLE-APPS track

### 11.1 共通

- `apps/shared/AccountSession.swift`：新增 `func sessionToken() async throws -> String`（ClerkKit 1.5.6 的 session token API，以實際 SDK 名稱為準）。
- 設定檔：`ClerkConfig.plist` 改名為 `AccountConfig.plist`，keys `clerkPublishableKey`、`cloudURL`（目前填 staging URL）；env `CODYNC_CLOUD_URL` 可覆寫。兩個 App 都要。
- `project.yml`：`MARKETING_VERSION: 3.0.0`；iOS、Widgets、NotificationService 加 `keychain-access-groups: [$(AppIdentifierPrefix)com.pokai.Codync]`，Info.plist 加 `CodyncKeychainGroup`；新增 `NotificationService` app-extension target（`apps/ios/NotificationService/`，bundle id `com.pokai.Codync.ios.NotificationService`，依賴 CodyncKit，embed 進 iOS App）；macOS **不加** entitlements（§3.2）；改完執行 `xcodegen generate`。
- `.github/workflows/release-macos.yml`：配合 `AccountConfig.plist` 改名。

### 11.2 iOS

- `AppStore` 改持有 `AccountStore`（取代單一 `BotStore`）；`RootView` 以 `accountStore.selection` 導覽，進入聊天時把 `accountStore.store(for: ref.computerId)` 注入 `ThreadView` 的 environment。
- Bot 清單：`accountStore.roster`，每列顯示 computer 名稱與其連線狀態（單台離線不影響其他）。
- 新增 bot：先選 computer（只列 online 的），再開 `BotEditorView`（用該 computer 的 store）。
- Computers 頁（取代 `SettingsView` 的電腦區）：本機配對與帳號電腦合併（依 `computerId`）；每台分別顯示「已加入帳號／已授權／可連線（直連／中繼）」；未授權的帳號電腦顯示「申請存取」→ 顯示 SAS 並輪詢；支援移除、改色、撤銷（帳號）。
- Pairing：`AccountStore.pair`；文案改為「不需要 Tailscale；在家用 Wi-Fi 直連，其他地方透過加密中繼」，Tailscale 步驟移為選配說明。
- 推播：`PushRegistrar` 對每個 `onConnected` 的 store 呼叫 `registerDevice {ticket, name, relay, pushKey: identity.pushKey, ctx: storage.id}`；通知點擊以 `userInfo["ctx"]`、`["computerId"]`、`["botId"]` 組 `BotReference`，只有 `ctx == storage.id` 且 computer 屬於目前 context 才導覽。
- NotificationService：`DeviceIdentity.openPush(sealed:computerId:contextID:)` 成功 → 替換 title／body；失敗保留通用文字。30 秒內完成，不做網路。
- Widgets：從 `SharedStore.Context` snapshot 顯示（`bots` 為 `[BotSnapshot]`）；Usage widget 只顯示 `usage[lastComputerId]`，重新整理時只對 `lastComputerId` 那台 `HostConnector.connect` + `usage` 單次呼叫（直連或中繼），失敗用快取。Live Activity 只顯示狀態（沒有 activity 文字）。deep link 用 `botURL(BotReference)`。
- 登入後：`CloudClient.registerDevice` → `refreshCloud`。登出／切換：`retire()`、刪該 context 的 device key 與快取（§3.2）。

### 11.3 macOS

- `HostController` 保留本機 host 管理；本機 store 以 `.loopback(baseURL: 127.0.0.1:port, token:)` attach 到 Mac 的 `AccountStore`。Mac 也有自己的 device key，可經 channel 連帳號中的其他電腦。
- 核准介面：本機（與每條 SSH）store 的 `accessRequests` 非空 → 選單列圖示加提示、跳出核准 sheet（裝置名稱、平台、email、SAS 大字、Approve／Deny 文字按鈕；`code == nil`（對方尚未揭露）時顯示等待中、Approve 停用）。
- 帳號：「將這台 Mac 加入帳號」（claim 流程 §4.2 A）、「移出帳號」（`unclaim`）、「已授權裝置」清單與撤銷（`devices`／`revokeDevice`）、「可從任何地方連線」開關（`setCloud`），並顯示 `cloudStatus`。
- `ChatWindow` sidebar 顯示跨 computer bots，新增 bot 先選 computer。

### 11.4 SSH（`apps/macos/App/SSHTunnel.swift` 等）

- Profile（非秘密，存 UserDefaults）：`id`、`alias` 或 `hostname`、`port`、`user`、`identityFile?`、`remotePort`（預設 19222）、`computerId`（首次連線後記住）、`name`。
- 輸入驗證：hostname／alias `^[A-Za-z0-9._-]+$` 且不以 `-` 開頭；user `^[A-Za-z_][A-Za-z0-9._-]*$`；port 1–65535；identityFile 必須是存在的檔案。以 `Process` 的 argv 陣列傳遞，**永不**經 shell 組字串。
- App 的 known_hosts：`~/.codync/ssh_known_hosts`（路徑無空白）。每個 ssh 呼叫都帶這組 argv（兩個元素；值內以**字面雙引號**包住絕對路徑，因為 ssh 自己的 config tokenizer 會以空白切開 `UserKnownHostsFile`，argv quoting 無效）：
  `-o`, `UserKnownHostsFile="<HOME>/.ssh/known_hosts" "<HOME>/.codync/ssh_known_hosts"`，以及 `-o`, `StrictHostKeyChecking=yes`。
- 解析目標：先跑 `ssh -G [-p port] [-l user] -- <dest>`，取 `hostname`、`port`、`hostkeyalias`、`proxyjump`、`proxycommand`。known_hosts 查詢名稱 = `hostkeyalias` 或 `hostname`，port ≠ 22 時寫成 `[name]:port`。
- Host key 判定（不解析 ssh 的 stderr 文字）：
  1. `ssh-keygen -F <name> -f <file>` 對兩個 known_hosts 檔各查一次。
  2. 找到 → 直接連線（`StrictHostKeyChecking=yes` 由 ssh 自己把關）。
  3. 沒找到且未設 `proxyjump`／`proxycommand` → `ssh-keyscan -p <port> -- <hostname>`，以 `ssh-keygen -lf -` 顯示指紋請使用者確認 → 以 `<name>` 為主機名寫入 `~/.codync/ssh_known_hosts`。
  4. 沒找到但設了 proxy → 不 keyscan，提示「請先在終端機以 ssh 連線一次以確認主機金鑰」（見 §0 取捨）。
  5. 找到了但連線以 exit 255 失敗、且 keyscan（無 proxy 時）取得的金鑰都不在已記錄的條目中 → 判定為**主機金鑰已變更**，阻擋並說明，不提供略過。
- 取得遠端資訊：`ssh <opts> -- <dest> "sh -lc 'codync-host info --json'"`（固定字串，無使用者輸入）。解析失敗 → 「遠端未安裝 codync-host」並顯示安裝指令（複製按鈕；不自動安裝）。
- Tunnel：`ssh -N -T -o ExitOnForwardFailure=yes -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o ForwardAgent=no -o ForwardX11=no -o BatchMode=yes -o StrictHostKeyChecking=yes -o 'UserKnownHostsFile="…" "…"' -L 127.0.0.1:<localPort>:127.0.0.1:<remotePort> [-p port] [-i key] [-l user] -- <dest>`；localPort 以綁定 port 0 取得。監看 process，斷線以 1→30 s 退避重啟；`computerId` 與 profile 記錄不符 → 停止並警告。
- 連上後：`AccountStore.attach(computer, route: .loopback(baseURL: 127.0.0.1:localPort, token: token))`；token 每次連線以 `info --json` 取得，不持久化。
- 多個 bots 共用一條 tunnel；只有 detach／移除 profile 才終止。
- 遠端 host 也可自行連雲端中繼：Mac 經 tunnel 呼叫 `pairing` 顯示 QR 給手機、或 `claimSign` 將遠端加入帳號。

### 11.5 APPLE-APPS 測試

編譯 iOS／macOS／Widgets；UI 流程手動驗收（§12.2）；SSH argv 組裝（含 `UserKnownHostsFile` 的字面雙引號與 `ssh -G` 輸出解析）與輸入驗證寫成可測的純函式並附單元測試（放在 `kit/Tests` 以外時，至少在 DEBUG 啟動時跑 `assert`-based self-check）。

---

## 12. 跨 track 驗證

### 12.1 整合測試（CLOUD 擁有：`cloud/test/e2e/relay.e2e.ts`，`npm run e2e`）

1. `wrangler d1 migrations apply codync-dev --local --env dev`；測試產生 RSA 金鑰，將公鑰 PEM 設為 `CLERK_JWT_KEY` 啟動 `wrangler dev --env dev --port 8787`。
2. 以 `CODYNC_HOME=<tmp> CODYNC_CLOUD_URL=http://127.0.0.1:8787` 啟動 `host/target/debug/codync-host serve --bind 127.0.0.1 --port <free>`。
3. 等 D1 出現 computer 列、DO presence online。
4. TS 假手機：loopback `pairing` 取 URL → 以 `pair=<offerId>` 走中繼 → `pair` → `hello` → `createBot`（backend `custom`，command 為 `python3 host/tests/fake_agent.py`）→ `events` 訂閱 → `send` → 收到 agent 回覆。
5. 關閉 host → device 收到 `presence online:false` → `mbox.put` 一則 `send` → `mbox.cancel` 另一則 → 重啟 host → 收到 `mbox.delivered`、events 出現該 clientNonce 的 entry、被取消的不存在。
6. 帳號流程：自簽 JWT → `POST /v1/claims` → loopback `claimSign` → complete → 第二支假手機 `POST /v1/devices` → access request（commit）→ 等 hostNonce → reveal → loopback `accessRequests` 的 `code` 等於手機算出的 SAS → `decideAccessRequest` → 以中繼連線成功 → `DELETE …/grants/{id}` → 1 秒內 socket 以 `4003` 關閉 → host 重連後（state 拉取前後）該 dk 都無法再連。
6b. 雲端惡意注入：直接在 D1 插入一筆 grant（第三把 key）→ host 拉 state 後該 key 仍被拒、ACL 不含它。
6c. pairing 經中繼：`pair` 回應後 link 以 `4100` 關閉，假手機以一般模式重連成功，且可超過 16 則訊息。
7. 直連：同一假手機以 `ws://127.0.0.1:<port>/channel` 握手成功；以非 loopback 位址用 bearer token 呼叫 `/api/hello` → 401。

### 12.2 手動驗收（owner + 實機）

兩個 Google 帳號、兩支 iPhone（或模擬器 + 實機）、一台 Mac、一台 Linux（SSH）。

---

## 13. 驗收條件

- 手機在行動網路（無 Tailscale）可與家中 NAT 後的 Mac 聊天、stop、回應權限、開 remote screen 訊號；`cloud` Worker log、DO storage 與 `relay/` 推播 Worker 收到的 body 中都找不到任何明文訊息、bot 名稱、token 或 pairing code。
- 同一 Wi-Fi 下自動走直連（UI 顯示直連）；離開 Wi-Fi 後在 10 秒內改走中繼且不重複訊息。
- 帳號裝置在電腦離線超過 15 分鐘後仍看得到「電腦離線」並可排入 mailbox。
- 電腦睡眠：手機顯示「電腦離線（上次 …）」而不是「連不到」；送出的訊息顯示「等待電腦上線」可取消；電腦醒來後依序送出、每則只執行一次；24 小時後未送出者顯示失敗。
- 未登入：QR 配對後同樣可經中繼使用；登入不會自動宣告本機配對的電腦。
- 帳號：新手機申請 → Mac 顯示相同 SAS → 核准 → 可連線；撤銷後 5 秒內（host 在線）斷線，host 連不到雲端時最長 15 分鐘失效；手機 B 不受影響。
- 另一帳號猜到 computerId／requestId／grantId 仍全部 404。
- 非 loopback 的 bearer token 一律 401；`setScreenEnabled`、`pairing`、`claimSign` 等經 channel 呼叫一律 403。
- 多 computer：本機 bot 與 SSH bot 並列，兩台有相同 bot ID 時 send／stop 不會送錯；SSH host key 變更時阻擋。
- CI：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`；`cloud`：`npm run typecheck && npm test`；`swift test`（kit）；Xcode 三個 target 可編譯。

---

## 14. 部署（staging 可由實作者執行；production 需 owner 確認）

### 14.1 Staging

```bash
cd cloud && npm ci
npx wrangler d1 create codync-staging          # 將 database_id 填入 [env.staging]
npx wrangler d1 migrations apply codync-staging --env staging --remote
npx wrangler secret put CLERK_SECRET_KEY --env staging      # owner 輸入 sk_test_…
npx wrangler secret put CLERK_WEBHOOK_SECRET --env staging  # owner 輸入 whsec_…
npx wrangler deploy --env staging               # → https://codync-cloud-staging.<account subdomain>.workers.dev
```
Host 測 staging：`codync-host cloud --url https://codync-cloud-staging.<sub>.workers.dev`（或 env `CODYNC_CLOUD_URL`）。Apple：`AccountConfig.plist` 的 `cloudURL`。

### 14.2 Secrets 與 vars

| 名稱 | 種類 | 說明 |
|---|---|---|
| `CLERK_SECRET_KEY` | secret（owner 輸入） | `verifyToken` 取 JWKS 用 |
| `CLERK_WEBHOOK_SECRET` | secret（owner 輸入） | Svix 簽章 |
| `CLERK_ISSUER` | var | Clerk Frontend API URL |
| `CLERK_AUTHORIZED_PARTIES` | var（選填） | 逗號分隔 |
| `CLERK_JWT_KEY` | var（只給 dev/e2e） | networkless 驗證用的公鑰 PEM；staging／production 不設 |

### 14.3 Owner 手動步驟

0. 允許實作者建立 staging 資源：`wrangler login` 已登入的 Cloudflare 帳號（或提供 `CLOUDFLARE_API_TOKEN`，權限 Workers Scripts:Edit、D1:Edit、Durable Objects）。
1. Clerk Dashboard（staging = `sunny-mollusk-8651`）：確認 iOS native app（`com.pokai.Codync.ios`）已註冊並允許 `com.pokai.Codync.ios://callback`；**Sessions → Customize session token** 加入 `{"email": "{{user.primary_email_address}}"}`；**Webhooks** 新增 `https://codync-cloud-staging.<sub>.workers.dev/v1/webhooks/clerk`，訂閱 `user.deleted`，複製 signing secret。
2. 輸入上面兩個 secrets。
3. Production：另建 Clerk production instance、`wrangler d1 create codync`、填 `wrangler.toml` 頂層、secrets 不加 `--env`、`npx wrangler deploy`；把 production URL 填入 `host/src/cloud.rs` 的 `DEFAULT_CLOUD_URL` 與 Release 版 `AccountConfig.plist`。
4. Apple Developer：新增 `com.pokai.Codync.ios.NotificationService` App ID 與 keychain access group 後，讓 Xcode 自動簽章更新 provisioning profiles。
5. `relay/` 的 `mutableContent` 變更需重新部署推播 Worker（`cd relay && npx wrangler deploy`，現有 production；需 owner 確認）。

---

## 15. 風險與未決

| 風險 | 說明／緩解 |
|---|---|
| 帳號流程的 host 公鑰由雲端傳遞 | 被攻破的 Worker 可換公鑰做 MITM；commit-then-reveal SAS 讓每次嘗試只有 10⁻⁶ 成功率，但前提是使用者真的比對。UI 必須把 SAS 放大並要求明確按 Approve。QR 流程不受影響。 |
| 推播 metadata | APNs／`relay/` 看得到 botId、computerId、context id、kind 與時間；內容已封裝（D10）。 |
| Worker／DO 可見 metadata | computerId、deviceKey、連線時間、訊息大小與頻率、mailbox 的 clientNonce 與失敗碼。文件與隱私說明需如實寫。 |
| 任何人可產生金鑰並註冊 computer | register 依 IP 限流、`computers.status='blocked'` 可封鎖；費用告警需在 production 前設定。 |
| `identity.json` 被複製 | 兩台機器搶同一 DO → `4009 replaced` 互踢並記錄錯誤；不自動修復。 |
| ClerkKit／Workers 限制 | ClerkKit session token API 名稱、Workers Rate Limiting binding、WebSocket 1 MiB 上限需在實作時對照當時文件；chunk 256 KiB 預留空間。 |
| `hkdf`／`sha2` 版本 | 見 §9.1。 |
| 協定升級 | 手機↔host 升 major 3；2.x App 需要更新才能連 3.x host。 |
| Production URL 未定 | `DEFAULT_CLOUD_URL = None` 時雲端停用（只有 QR + 直連）；production 部署時填入 host 常數與 Release 設定檔。 |
| host 撤銷帳號裝置但 `POST /v1/host/grants/{id}/revoke` 失敗 | 本機已撤銷且 state 不能把它加回（§4.3），D1 只是顯示仍 granted；下次成功時補送。 |
| 直連 LAN 的 `ws://` | 內容已 E2E；但 metadata（連線、大小）在 LAN 可見，可接受。 |
