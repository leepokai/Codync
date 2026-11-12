<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.45.1
### zh-Hant
- 新增「將有新動態的機器人移到最前面」：在電腦的設定裡開啟後，有新訊息（你的訊息、bot 的回覆或 routine 執行）的 bot 會自動排到列表最上面。
- 在對話中尋找：點聊天右上角的放大鏡，輸入文字就會顯示有幾個符合項目，用上、下箭頭在它們之間移動，目前的訊息會被標示出來。搜尋的是已載入的訊息。
- bot 要你選擇時，聊天中會出現可點選的選項卡。選項會送回正在進行的工作，已回答或過期的卡片不會重複送出。

### en-US
- New "Move active bots to the top": turn it on in a computer's settings and a bot with a new message (yours, its reply, or a routine run) moves to the top of the list.
- Find in a conversation: tap the magnifying glass at the top right of a chat and type. The bar shows how many matches there are, the arrows move between them, and the message with the current match is highlighted. It searches the messages loaded so far.
- When your bot needs a choice, tap an option in its chat card. Your answer continues the current work; answered or expired cards cannot send another choice.

## 2.44.1
### zh-Hant
- 有重要更新時，更新後第一次開啟 Codync 會跳出「新功能」視窗，列出這個版本改了什麼。一般的更新不會顯示。

### en-US
- After an important update, Codync shows a What's new window the first time you open it, listing what changed. Most updates don't show one.

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
- New: Codync Cloud (beta). Signed in to a Codync account, start a cloud computer on your own Boat (EU) or InstaBox (US) account, so your bots work while your computer is off. Codync charges nothing; the provider bills your account under its own plan. In Manage computers, choose + in the Codync Cloud section.
- A cloud computer sleeps while idle and wakes when you message one of its bots. Up to three per account, each with its own name.
- No InstaBox account yet? Its free plan includes one machine: choose Get a free sandbox on InstaBox's key sheet.

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
- Use Codync without an account: choose Continue without an account on the welcome screen, then scan your computer's QR code. It connects over Wi-Fi or Tailscale. Sign in later and your computers stay paired; Add to account in Manage computers brings each into your account once the computer confirms.
- A computer can stay out of your account. It asks when you pair; without it, your phone connects only over Wi-Fi or Tailscale, and you can add it later in Manage computers.
- Remote screen now follows the Connection setting: Auto tries direct first, then Cloudflare, and Cloudflare first does the reverse.
- Typing @ in a group lists its members one per row, like the / command list.

## 2.40.3
### zh-Hant
- bot 現在可以讓 dev server 持續在電腦上執行，不會因為 bot 閒置或重新啟動而中斷。port 會顯示在聊天標題下方；點 bot 的名稱，就能看記錄、重新啟動或停止；手機連得到電腦時，也能直接開啟。
- 用量頁面可以自己選要看哪些服務並調整順序：預設只顯示 bot 正在用的，也可以打開或隱藏其他服務，或拖曳排列。另外支援顯示 Google Antigravity 和 GitHub Copilot 的額度。
- 修正 bot 更新時，選單偶爾點了沒反應的問題。
- 修正 iOS 27.2 之後的部分測試版上，登入後 app 當掉，之後每次開啟都立刻關閉（重新安裝也一樣）的問題。

### en-US
- Bots can now keep a dev server running on your computer, even when the bot goes idle or restarts. Its port shows under the chat title; tap the bot's name to read its logs, restart or stop it, and to open it when your phone can reach it.
- Choose and arrange which agents show in Usage: by default it lists the ones your bots run, and you can turn others on, hide them or drag them into order. Usage now also reads Google Antigravity's and GitHub Copilot's limits.
- Fixed menus sometimes ignoring a tap while a bot was updating.
- Fixed a crash on some betas of iOS 27.2 and later that closed the app after sign-in and at every launch afterwards, even after reinstalling.
