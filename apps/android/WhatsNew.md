<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.45.1
### zh-Hant
- 新增「將有新動態的機器人移到最前面」：在電腦的設定裡開啟後，有新訊息（你的訊息、bot 的回覆或 routine 執行）的 bot 會自動排到列表最上面。

### en-US
- New "Move active bots to the top": turn it on in a computer's settings and a bot with a new message (yours, its reply, or a routine run) moves to the top of the list.

## 2.44.1
### zh-Hant
- 有重要更新時，更新後第一次開啟 Codync 會跳出「新功能」視窗，列出這個版本改了什麼。一般的更新不會顯示。
- 修正 Codync 在 bot 很多或聊天紀錄很長時一開啟就自動關閉的問題：現在每個聊天只會先載入最新的訊息，較舊的訊息往回捲動時再載入。

### en-US
- After an important update, Codync shows a What's new window the first time you open it, listing what changed. Most updates don't show one.
- Fixed Codync closing by itself right after opening with many bots or very long chats. Each chat now loads its newest messages first, and older ones load as you scroll back.

## 2.43.2
### zh-Hant
- bot 頭像的點變大了：大頭像也改用 9 × 9 的大點，看起來更清楚。

### en-US
- Bot avatars have larger dots: large avatars now use the same 9 × 9 grid of big dots, so they read more clearly.

## 2.43.1
### zh-Hant
- 新功能 Codync Cloud（Beta）：登入 Codync 帳號後，可以在你自己的 Boat（歐洲）或 InstaBox（美國）帳號上開雲端電腦，你的電腦沒開，bot 也能工作。Codync 不收費，機器由供應商依它的方案向你的帳號收費。在「管理電腦」的 Codync Cloud 區塊按 +。
- 雲端電腦閒置時會睡著，傳訊息給它的 bot 就會醒來。每個帳號最多三台，各自命名。
- 還沒有 InstaBox 帳號？它的免費方案有一台機器，在 InstaBox 的 key 頁面點「Get a free sandbox」。

### en-US
- New: Codync Cloud (beta). Signed in, start a cloud computer on your own Boat (EU) or InstaBox (US) account, so bots work while your computer is off. Codync charges nothing; the provider bills you under its plan. Manage computers, then + under Codync Cloud.
- It sleeps while idle and wakes when you message one of its bots. Up to three per account.
- No InstaBox account yet? Its free plan includes one machine: choose Get a free sandbox on InstaBox's key sheet.

## 2.42.2
### zh-Hant
- 第一次用 Google、GitHub 或 Apple 登入的新使用者，現在能直接建立帳號；之前會回到歡迎畫面，帳號沒有建立。
- 登入到一半離開登入頁面，不會再跳出「Error occurred with unknown message.」；真的登入失敗時，會提示檢查網路後再試一次。

### en-US
- New users signing in with Google, GitHub or Apple for the first time now get their account. Before, sign-in returned to the welcome screen without creating it.
- Leaving the sign-in page partway no longer shows "Error occurred with unknown message." A sign-in that really fails asks you to check your connection and try again.

## 2.42.1
### zh-Hant
- bot 詳情「智慧體」區塊的「壓縮對話」預設改為「自動」：交給 Claude Code 或 Codex 在對話快滿時自己壓縮。想讓 Codync 提早壓縮，可以改選提早、接近上限或指定數量（從 100K、200K… 裡挑，或自己輸入）。已經是「提早」的 bot 會保留原設定，想交給智慧體自己壓縮可以改選「自動」。

### en-US
- Summarize long chats, in a bot's Agent section, now defaults to Auto: Claude Code or Codex summarizes a chat on its own once it fills up. Pick Early, Near the limit or a size (100K, 200K and so on, or one you type) to have Codync summarize sooner. Bots already on Early keep it; pick Auto to leave it to the agent.

## 2.42.0
### zh-Hant
- bot 詳情的「智慧體」區塊新增「壓縮對話」：選擇 Codync 什麼時候把長對話壓縮成摘要。可以選提早（預設）、接近上限、不主動壓縮，或指定數量：從 100K、200K… 裡挑，或自己輸入，最多到模型裝得下的上限。適用於 Claude Code 和 Codex 的 bot。

### en-US
- New Summarize long chats setting in a bot's Agent section: choose when Codync summarizes a long chat. Pick Early (default), Near the limit, Only when full, or a size: 100K, 200K and so on, or one you type, up to what the model holds. For Claude Code and Codex bots.

## 2.41.0
### zh-Hant
- 不登入也能用了：在歡迎畫面選擇「不登入繼續」，手機掃電腦的 QR 碼就能配對，透過 Wi-Fi 或 Tailscale 連線。之後登入，配對會留在手機上；到「管理電腦」按「加入帳號」，電腦確認後就能從任何地方連線。
- 電腦可以不加入帳號。配對時電腦會詢問；不加入的話，手機只能用 Wi-Fi 或 Tailscale 連線，之後也能在「管理電腦」加入帳號。
- 遠端畫面會照連線設定換路：自動是先直連再走 Cloudflare，Cloudflare 優先則相反。
- 群組裡輸入 @ 會列出成員，一排一個，和 / 指令選單一樣。

### en-US
- No account needed: choose Continue without an account, then scan your computer's QR code to connect over Wi-Fi or Tailscale. Sign in later and your computers stay paired.
- A computer can stay out of your account: it asks when you pair, and Manage computers can add it later.
- Remote screen now follows the Connection setting: Auto tries direct first, then Cloudflare, and Cloudflare first does the reverse.
- Typing @ in a group lists its members one per row, like the / command list.

## 2.40.2
### zh-Hant
- bot 現在可以讓 dev server 持續在電腦上執行，不會因為 bot 閒置或重新啟動而中斷。port 會顯示在聊天標題下方；點 bot 的名稱，就能看記錄、重新啟動或停止；手機連得到電腦時，也能直接開啟。
- 用量頁面可以自己選要看哪些服務並調整順序：預設只顯示 bot 正在用的，也可以打開或隱藏其他服務，或拖曳排列。另外支援顯示 Google Antigravity 和 GitHub Copilot 的額度。

### en-US
- Bots can now keep a dev server running on your computer, even when the bot goes idle or restarts. Its port shows under the chat title; tap the bot's name to read its logs, restart or stop it, and to open it when your phone can reach it.
- Choose and arrange which agents show in Usage: by default it lists the ones your bots run, and you can turn others on, hide them or drag them into order. Usage now also reads Google Antigravity's and GitHub Copilot's limits.

## 2.39.0
### zh-Hant
- 第一次設定時，如果帳號裡已有電腦，可以向它申請訪問，或配對一臺新電腦（例如 VPS）。安裝頁也能一鍵複製網站連結，到電腦上下載。
- bot 工作中送出的訊息會排在輸入框上方，可立即送出或收回。
- bot 用了不只一種智慧體時，列表會在頭像左下角標出各自的智慧體標誌，可在「設定 › 外觀」關閉。
- 修正與電腦的連線中斷時，app 可能閃退的問題。
- 語音通話中語音辨識偶爾出錯時，會送出已聽到的內容並繼續聆聽，不再直接結束通話。
- 介面新增巴西葡萄牙文：可在設定選擇「Português (Brasil)」，或跟隨系統的葡萄牙文語言。
- 新增俄文介面：在設定選擇「Русский」，或跟隨系統的俄文語言。
- 用 QR Code 配對的電腦如果當下沒能加入帳號，下次連上時會自動補上。
- 語音通話的對話紀錄更好讀，通話列在 bot 沒說話時也會顯示一排靜止的點。
- 排隊狀態、例行任務和執行紀錄會跟隨介面語言，名稱與訊息保留原文。
- 新增義大利文介面：可在設定選擇「Italiano」，或跟隨系統的義大利文語言。
- 擴充功能的「已安裝」提示也會顯示技能的圖示。

### en-US
- Messages sent while a bot works wait above the box: send now or take back.
- First-time setup offers your account's computers or a new one.
- When bots use different agents, the list shows each one's logo (off in Settings).
- Fixed a crash when the connection dropped.
- A one-off speech error no longer ends a voice call.
- Choose Português (Brasil), Русский or Italiano in Settings.
- A QR-paired computer joins your account when it next connects.
- Calls read more clearly.
