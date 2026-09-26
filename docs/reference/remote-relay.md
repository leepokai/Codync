# Remote relay protocol

Reviewed 2026-09-26. This reference preserves the channel's numbered wire-contract sections. It describes the implemented architecture; it is not a deployment acceptance report. Historical work assignments and proposed SDK definitions have been replaced by source entry points.

Shared deterministic [vectors](fixtures/remote-relay-vectors.json) and their [generator](fixtures/remote-relay-vectors.py) are consumed by Rust, Swift and TypeScript; see [fixture maintenance](fixtures/README.md).

## 0. 決策摘要

| # | 決策 | 理由 |
|---|---|---|
| D1 | **Cloudflare 中繼是離家連線的主要路徑**；LAN／Tailscale 直連是替代：client 先平行試直連（1.5 秒內完成握手就用），否則走中繼。Tailscale 永遠不是必要條件。 | 使用者不用裝 VPN、不用開 port。 |
| D2 | **端對端加密（locked box）**。直連和中繼**用同一套 channel 協定**：Ed25519 簽章的臨時 X25519 ECDH → HKDF-SHA256 → ChaCha20-Poly1305。Worker／DO 只看得到密文與路由 metadata。 | 一套協定、一份測試向量；LAN 上也不再有明文 token。 |
| D3 | 每台 computer 一個 Durable Object（SQLite storage、WebSocket Hibernation）：presence、轉送密文、離線 mailbox（手機→電腦，24h TTL，可取消，依序投遞，ack 後刪除）。電腦→手機的補送仍由 host 的 `rev`／events 語意負責。 | 符合產品決策；DO 不保存聊天。 |
| D4 | **Host 是授權的唯一權威**。host 在本機 SQLite 保存「已授權裝置」表；**只有兩條路能新增列**：本機 QR 配對（§4.1）與 host 自己的核准動作（`decideAccessRequest` approve，§4.2 B）。雲端 state **只能縮短**授權（刪除列、延長既有列的 lease），永遠不能新增。host 把此表簽成 **ACL** 發佈到自己的 DO，DO 據此放行 WebSocket；E2E 握手時 host 再驗一次。 | 無帳號模式也能用中繼（host 以機器金鑰向雲端註冊，不需帳號）；雲端被攻破也無法新增可解密的裝置（新增要 host 本機的使用者動作、ACL 要 host 簽、握手要 host 私鑰）；撤權有兩道即時關卡（DO 封鎖 + host 關 channel）。 |
| D5 | **Computer ID 由 host 簽章公鑰推導**：`computerId = b64url(SHA-256(hostSignPub)[0..16])`。不由雲端分配。 | 自我驗證、離線可得、無法被搶註；DO 名稱可直接推導。代價：換 host 金鑰 = 新 computer（見 §3.5）。 |
| D6 | 帳號 → Computers → Bots：`AccountStore`（`[ComputerID: BotStore]`），`BotReference = accountId + computerId + botId`。 | 依帳號與電腦隔離狀態。 |
| D7 | 共用 bearer token（`~/.codync/token`）**只接受 loopback 連線**（Mac App 本機、SSH tunnel、statusline、MCP、TUI、Linux App）。手機與其他遠端 client 一律走 E2E channel。 | 移除「一個 token 開所有手機」的舊模型；不留相容路徑。 |
| D8 | SSH：macOS App 以系統 OpenSSH `-L` 轉發到遠端 host 的 loopback API；遠端側看到的是 loopback 呼叫者，所以 SSH 帳號等同本機使用者權限（能讀 `~/.codync/token` 的人本來就有這權限）。 | 產品決策 6；重用 loopback API，不需要額外授權層。 |
| D10 | **推播內容也加密**：host 以裝置註冊的 X25519 push key 封裝通知標題／內文（§6.7），APNs 只帶通用文字 + `mutable-content`，iOS Notification Service Extension 解開。Live Activity 只推狀態 enum，不推自由文字。 | 決策 2：Cloudflare（`relay/`）只看得到密文。 |

**審查後的取捨（rev 2）**：
- 撤權後的 DO 解封鎖以 **grantId** 判斷（新 grant 或本機配對的 null grant），不比較時間戳（見 §7.3）；比 `stateAt` 方案少一個跨時鐘欄位。
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


## 2. Implementation map

| Area | Source |
| --- | --- |
| Worker API / authentication / DO | `cloud/src/api.ts`, `auth.ts`, `relay.ts` |
| Cloud schema | `cloud/migrations/` |
| Host identity / crypto / channel | `host/src/identity.rs`, `crypto.rs`, `channel.rs` |
| Host cloud / relay | `host/src/cloud.rs`, `relay.rs` |
| Swift transports / identity | `kit/Sources/CodyncKit/Client/` |
| Account and per-computer state | `kit/Sources/CodyncUI/` |
| Apple account integration | `apps/shared/AccountSession.swift` |

See [architecture](../architecture/overview.md) and [file structure](../architecture/file-structure.md). Application/host compatibility follows their major version; channel `v`, QR version and Worker package version are separate values.

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
2. `GET /v1/computers`（帶 `Codync-Sig` 則回傳每台的 `access`）。 iPhone 的 Bots 首頁直接列出尚未連線的帳號電腦，點 **Connect** 進入存取申請，不需掃 QR Code；核准後顯示該電腦的 bots。首次裝置授權仍由電腦上的使用者確認。
3. 對 `access == "none"` 的電腦：手機產生 32 random bytes `nD`（只存記憶體），`POST /v1/computers/{id}/access-requests {"commit": b64url(SHA-256("codync/sascommit/v1" ‖ dk ‖ nD))}` → `{requestId, expiresAt}`。
4. Worker 通知 DO → host 拉 state → 看到新申請（含 `commit`）→ host 產生 32 random bytes `nH`（存本機記憶體，每個 requestId 只產生一次）→ `POST /v1/host/access-requests/{id}/nonce {"nonce": nH}`（已有 host nonce → `409 conflict`，host 不得換）。
5. 手機輪詢 `GET /v1/access-requests/{id}`（每 2 秒）→ 出現 `hostNonce` 後才 `POST /v1/access-requests/{id}/reveal {"nonce": nD}` → Worker 存入並通知 DO `cloud.changed`。手機此時計算 SAS 顯示「在電腦上確認代碼 123456」。
6. host 拉 state 取得 `deviceNonce` → 驗 `SHA-256("codync/sascommit/v1" ‖ dk ‖ nD) == commit`（不符 → 自動 deny 並記 `warn`）→ 計算 SAS → `accessRequests` 事件 → Mac App 顯示核准對話框（裝置名稱、平台、帳號 email、SAS）。**`nD` 驗證通過前 host 不顯示 SAS、不接受 approve**（`decideAccessRequest` 回 409）。無 GUI 的 host：`codync-host access list|approve|deny`。
7. 核准 → host `POST /v1/host/access-requests/{id}/decision {"decision":"approve"}` → D1 建立 grant → 回 `grantId` → host **以自己顯示給使用者的 `dk` 與回傳的 `grantId`** 新增裝置列（`source=account`，lease 15 分鐘）→ 發佈 ACL。
8. 手機輪詢到 `approved` → 連線（§7.5）。第一次 `hello` 回應帶 `boxKey`，此時 pin（§3.3）。

**SAS**：`sas = uint32_be(SHA-256("codync/sas/v2" ‖ hostSignPub ‖ dk ‖ nD ‖ nH)[0..4]) mod 1_000_000`，左補零成 6 位數（向量 `sas`）。手機用從雲端拿到的 `signKey` 算、host 用自己的公鑰與申請內 dk 算。因為雙方的 nonce 在對方的值確定之後才揭露，被攻破的雲端每次申請只有 10⁻⁶ 的機會讓兩邊代碼相同，無法離線搜尋金鑰。手機核准後 pin 住算 SAS 用的那把 `signKey`，之後以 `welcome.sig` 驗證。

### 4.3 授權表與 ACL（host 為權威）

host 的授權裝置表（`host/src/devices.rs`）每一列：`key, name, platform, source(local|account), grant_id, scopes, lease_until`。

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
<authority：小寫的 Host header 值，含非預設 port，例如 codync-cloud-dev.x.workers.dev 或 127.0.0.1:8787>
<path + query，與實際送出的 request-target 逐字相同>
<ts>
<nonce>
<b64url(SHA-256(body bytes))>   ← 無 body（GET、WS）用空字串的 hash
```
驗證：`|now - ts| ≤ 300_000`；`(kid, nonce)` 在 10 分鐘內未出現過（HTTP：D1 `sig_nonces`；relay：DO 的 `nonces` 表）；Ed25519 驗證（Worker 用 WebCrypto `Ed25519`）。Worker 以 `new URL(request.url).host`（小寫）比對 authority，所以 dev 簽的 header 在 production 無效。失敗 → `401 {"error":{"code":"badSignature"}}`。向量：`requestSig`。

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
| loopback 專用 | `setScreenEnabled`, `pairing`, `computerCall`, `teamCall`, `claimSign`, `unclaim`, `devices`, `revokeDevice`, `accessRequests`, `decideAccessRequest`, `cloudStatus`, `setCloud` | `Caller::Local`（loopback + token） |
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
- 一般 socket 的 `dk` 在 `blocked` 表 → `403`；pairing socket 仍可申請配對。
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
| `acl` | `d`（ACL JSON bytes 的 b64url）, `sig` | DO 以 host kid 驗簽、`computerId` 必須相符 → 否則 `{"t":"error","code":"badAcl","ver":<已存>}`。`ver` ≤ 已存 ver → `{"t":"error","code":"staleVersion","ver":<已存>}`，host 設 `acl_ver = max(本機, ver) + 1` 重新簽發。成功 → 存起來；以 `4003 revoked` 關閉 dk 不再列在 `devices` 的一般 socket；以 `4410 pairingExpired` 關閉其 offer 不再列在 `offers` 的 pairing socket；**解除封鎖在**新 ACL 以「null（本機配對）或不同的 `grant`」列出被封鎖的 dk 時（即 host 做了一次新的核准；`blocked` 表存 `(dk, grant_id)`）；回 `{"t":"acl.ok","ver":N}` |
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
5. 每次 inner `hello` 成功後，把回應的 `urls`、`cloud`、`device`、`name`、`boxKey` 合併進持久化的 `Computer`（§3.3、§10）；帳號來源的 computer 因此在第一次中繼連線後也能試 LAN 直連。

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

## 8. Cloud HTTP API

The route table below summarizes the contract. Executable schema and validation live in `cloud/migrations/` and `cloud/src/api.ts`; do not maintain a second copy of the schema here.

### 8.4 `/v1` HTTP API

所有 body 為 JSON；時間為 ms。「Clerk」= 需要 Bearer；「Sig(dev)」= 需要該 device key 的 `Codync-Sig`；「Sig(host)」= host key 的 `Codync-Sig`。

**公開**
- `GET /v1/health` → `{"ok":true,"version":"2.2.0"}`

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
- `POST /v1/host/access-requests/{id}/nonce` `{"nonce"}` → `{}`（`cloud/src/api.ts`；已設過 → `409`）
- `POST /v1/host/grants/{grantId}/revoke` `{}` → `{}`（host 在本機撤銷帳號來源裝置時同步 D1，`revoked_reason='owner'`；失敗記錄並存入 `grant_revokes` 重試，本機撤銷已生效）
- `POST /v1/host/access-requests/{id}/decision` `{"decision":"approve"|"deny"}` → `{"status","grantId"?}`（`cloud/src/api.ts` 的原子更新；request 過期 → `410 requestExpired`；approve 要求 `device_nonce IS NOT NULL`，否則 `409`）
- `POST /v1/host/unclaim` `{}` → `{}`（在電腦上移出帳號；同 `DELETE /v1/computers/{id}` 的效果）

**Relay**：§7.1。**Webhook**：`user.deleted` → accounts `status='deleted'`、devices 撤銷、grants 撤銷（`accountDeleted`）、computers owner=NULL、DO block + changed。

## 9. Host integration

`host/src/api.rs` dispatches inner methods and enforces caller permissions. Bearer HTTP/SSE is loopback-only; remote callers use the encrypted channel. `host/src/cloud.rs` manages registration, claims, access state and grant revocation retries; `host/src/relay.rs` manages the outgoing relay connection. Inspect these implementations for current method signatures and storage keys.

`codync-host cloud` reports cloud status; `devices` lists/revokes device access; `access` handles pending approval. `reset-token` rotates the local bearer credential, not remote device grants.

## 10. Shared Swift integration

`CodyncKit/Client` owns transport, crypto, identity and cloud API models. `CodyncUI` owns account/per-computer stores and presentation. Consumers must retain account and computer scope when handling asynchronous responses, links and cached widget data. See [file structure](../architecture/file-structure.md).

## 11. Apple apps and SSH

The Apple apps share Clerk integration. QR pairing remains usable without an account; account sign-in requires host-approved device access. macOS uses OpenSSH forwarding for SSH computers, not a mobile SSH gateway. See [accounts and SSH](../guides/accounts-and-ssh.md).

## 12. Verification

### 12.1 Local integration

`cloud/test/e2e/relay.e2e.ts` exercises a real host with local Wrangler and a fake ACP agent. Run `npm run e2e` in `cloud/` after building the host. The three language implementations also consume the shared vectors.

### 12.2 Deployed-device verification

Use the [Cloudflare testing guide](../guides/cloudflare-testing.md). Local tests do not establish live OAuth, deployed Worker, APNs or background behavior.

## 13. Acceptance criteria

Verify pairing and approval, same-computer direct/relay connectivity, encrypted chat and reconnect catch-up, mailbox cancellation/deduplication, revocation, account isolation, and separate APNs delivery. Record actual results and routes. These are acceptance targets, not claims that all scenarios have passed.

## 14. Environments and deployment

See [environments and deployment](../guides/environments-and-deployment.md) and [cloud setup](../../cloud/README.md). Debug targets development. Main app configuration is empty and production Worker bindings include placeholders; production readiness must be established separately.

## Implementation clarifications

The following implemented clarifications take precedence over the original contract wording above:

- **Close code**：host 送出 `reject` 後自行挑選 WebSocket close code（`RejectCode::close_code`），不照 §表格逐一對應。
- **`cloud_synced`**：relay 斷線時不清除；帳號裝置的 lease 最長 15 分鐘內自然失效。
- **Access request**：host 無法完成的請求（例如缺 key、狀態不一致）自動 deny，不留在 pending。
- **Grant 撤銷**：失敗的撤銷記在 kv `grant_revokes`，之後重試。
- **`Identity::sign_request`** 多一個 `authority` 參數（小寫 Host header，含非預設 port），簽名涵蓋它。
- **DO admission**：規格未列的情況也可能回 `403` / `429`。
- **vitest**：`compatibilityDate` 固定 `2026-08-22`。
- **Kit relay 升級**：遇 `403` 重試最多 3 次。
- **SAS nonce**：host 每小時最多 5 個（kv `sas_nonces`）；每把 device key 同時只允許一個尚未 reveal 的請求。
- **直連 `reject`**：明文的直連 `reject` 不結束連線競速：仍會嘗試 relay，已存的 computer 持續重試。
- **Mailbox 重試**：沿用同一個 `clientNonce`，但每次重新 seal。
- **ACL**：列表中 `grant` 為 null 的已封鎖 dk 會被解除封鎖；被封鎖的 dk 仍可開 pairing socket。
- **帳號裝置 mailbox**：第一次拉取 state 之前收到的 item 先保留、不 ack。
- **SSH forward**：以 `lsof` 確認本機 listener 屬於我們啟動的 `ssh`。
- **取消 access request**：iOS SAS sheet 與 Mac computers 視窗都有取消按鈕（`DELETE /v1/access-requests/{id}`）。

## 15. Limits and risks

- Account discovery relies on cloud-provided public keys; users must actually compare the commit/reveal six-digit approval code. QR pairing obtains host keys directly from the pairing payload.
- The cloud sees account/device records and routing metadata, connection times, sizes, frequency and mailbox identifiers. Channel content is encrypted. APNs has its own visible routing metadata.
- Copying `identity.json` creates competing connections for one computer identity; replacing host keys requires pairing again.
- Revocation is enforced locally even when cloud synchronization fails; pending cloud revocations are retried.
- Direct `ws://` still encrypts channel content, but exposes connection metadata to the local network.
- Mailbox retention is bounded and is not cloud transcript storage. TURN, phone-native SSH and team sharing are outside this contract.
