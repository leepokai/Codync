<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.47.0
### zh-Hant
- 應用程式分頁換成 CopilotKit 審核過的連接器目錄：Google Drive、Notion、Linear、Sentry、Exa、Firecrawl 等服務，各自保留原廠的授權方式。
- 需要登入的應用程式在瀏覽器完成授權，金鑰型的應用程式則填入自己的金鑰；兩者都只留在你的電腦上。
- 登入回來時會核對發出的服務，移除連接器時也會一併撤銷授權。
- 應用程式分頁也加入 Composio：在電腦上填一次 Platform 金鑰，就能瀏覽並連接 Gmail、Slack、GitHub、Notion 等 1,000 多個應用程式，金鑰只留在這台電腦上。

### en-US
- The Apps tab now lists CopilotKit's reviewed connector catalogue: Google Drive, Notion, Linear, Sentry, Exa, Firecrawl and more, each keeping the vendor's own sign-in or key.
- Apps that need a sign-in finish it in the browser, and key apps take your own key; both stay on your computer.
- A sign-in that comes back is checked against the service that issued it, and removing a connector revokes its grant.

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
- 擴充功能現在是市集：應用程式、代理程式與技能三個分頁，附精選項目與分類。
- 代理程式會以自己的指令、外觀、技能與例行工作建立 Bot；先讀它的頁面再新增。
- 技能隨 App 內建、可離線安裝；目錄中的應用程式一步就能加入對應的連接器。
- 新的 Workflow Builder 會把觸發條件、步驟與結果建成流程，步驟會依次出現在畫布上。
- 每個 Bot 可以在設定中命名瀏覽器設定檔，讓登入與 Cookie 保留在該設定檔裡。
- 可以從訊息選單刪除任何訊息；它會在所有裝置上消失。
- 每個 Bot 的聊天可以選擇顏色，方便一眼分辨。
- Slack、Telegram 與 Discord 的提及可以啟動流程；GitHub 的 issue、PR 與失敗檢查也一樣。
- 新增 TickTick 等應用程式，以及每個代理程式的建立者、審核狀態、分類與更新日期。
- WhatsApp 號碼可以用配對代碼連結，不必掃描 QR 碼。
- 中斷的任務與失敗的例行工作可以用「取消任務」結束，訊息會保留。
- 語音通話的停頓改成滑桿（0.6–3 秒），朗讀速度可選 0.5 倍到 2 倍。
- Bot 頭像可選絨感或扁平，靜止時的表情更放鬆，群組頭像會輕輕浮動。
- Proton Pass 加入目錄：填入個人存取權杖後，Bot 即可讀取它開啟的保險庫。
- Composio 的應用程式在擴充功能中列出並顯示連線狀態，捲動時會繼續載入。
- 被用完的方案額度擋住的模型會說明原因，以及額度恢復的時間。
- 長按對話可以標為未讀；重新開啟後會出現「新訊息」分隔線，對話的媒體與檔案也會列出。
- 連線不穩或系統清掉 App 之後會自行重新連上，省電模式也不再中斷串流中的回覆。
- 設定新增「支援」：Discord、GitHub 與可翻頁的支援記錄，記錄可以清除。
- 例行工作可以由 WhatsApp 訊息或群組提及啟動，也可以把 WhatsApp 對話當作結果。
- 流程可以接手 Slack 觸發條件與結果。
- 等待中的訊息會列在意見欄上方，可以改寫、直接插進目前的回覆，或刪除。
- 每段文字依自己的方向閱讀，程式碼一律由左至右；介面可以選阿拉伯文並由右至左排列。
- 可以用 PIN 或手機的指紋、臉部、螢幕鎖定鎖住 App，離開後依設定重新鎖上。
- 從其他 App 分享時，分享面板會直接列出釘選的代理程式。
- 頻道畫面會顯示這台電腦的 Telegram 機器人、Discord、Slack 與 GitHub；可以替機器人配對對象或把人移除。
- WhatsApp 現在有自己的一列，從擴充功能市集也能開啟連結畫面。
- 工作流程的連線可以指定目標機器人那一輪要用的模型與選項。
- 每個聊天都可以有討論串：從任何訊息或列表開啟，回覆留在討論串裡，聊天只顯示回覆數。
- 語音通話新增 ElevenLabs：使用你自己的金鑰，由 Scribe 聽你說話、帳號中的聲音讀出回覆；測試會唸出「OK」。
- Hermes 環境、記憶與子代理路由可一次套用。
- 通話可選 NeuTTS 本機串流語音，說話可中斷回覆；手機新增自選 Hey Hermes 前景喚醒。
- 擴充功能加入 Hermes 技能與 MCP 應用程式，標示來源；技能可離線安裝。
- Bot 可保留自己的技能改進，並依要求查看、還原版本。
- Telegram、WhatsApp、Slack、Discord 語音訊息沿用語音金鑰；例行工作可透過已連結的 WhatsApp 傳送報告。
- 市集的代理附技能與例行工作；Workflow Builder 可建立連動步驟。
- Bot 可使用獨立瀏覽器設定檔與聊天顏色；訊息刪除會同步。
- Slack、Telegram、Discord 與 GitHub 事件可啟動流程；新增 TickTick 等應用程式及代理資訊。

### en-US
- Extensions is now a marketplace with Apps, Agents and Skills, featured picks and categories.
- Adding an agent creates a bot with its instructions, look, skills and routines; read its page first.
- Bundled skills install offline, and catalogued apps add their connector in one step.
- The new Workflow Builder turns a trigger, steps and a result into a workflow that draws itself on the canvas.
- Each bot can name a browser profile, keeping sign-ins and cookies inside that profile.
- Any message can be deleted from its menu and disappears on every device.
- Each bot's chat can have its own color, so it is easy to tell apart.
- Slack, Telegram and Discord mentions can start a workflow, as can GitHub issues, pull requests and failed checks.
- New apps such as TickTick, and each agent page now names its creator, review, category and update date.
- A WhatsApp number can be linked by typing a pairing code instead of scanning a QR code.
- A task that stopped halfway, or a routine run that failed, can be ended with Cancel task; its messages stay.
- The voice call pause is a slider from 0.6 to 3 seconds, and read-aloud speed goes from 0.5× to 2×.
- Bot avatars can be plush or flat, resting faces look calmer, and group avatars drift gently.
- Proton Pass joins the catalog: add its personal access token and your bots can read the vault it opens.
- Composio's apps are listed in Extensions with their connection status and load as you scroll.
- A model stopped by a spent plan limit says so, and when the limit comes back.
- Long-press a chat to mark it unread; opening it again shows a New messages line, and the chat lists its media and files.
- Codync reconnects on its own after a flaky connection or the system stopping it, and battery saver no longer cuts a streaming reply.
- Settings has Support: Discord, GitHub and a paged support log you can clear.
- A routine can start from a WhatsApp message or group mention, and a WhatsApp chat can be a routine output.
- Workflows accept Slack triggers and outputs.
- Waiting messages show above the composer to rewrite, steer into the running reply, or delete.
- Each paragraph reads in its own direction and code always reads left to right; the interface comes in Arabic, right to left.
- Lock Codync behind a PIN or the phone's fingerprint, face or screen lock, and it locks again after the chosen time away.
- Sharing from another app offers your pinned agents in the share sheet.
- Channels shows the computer's Telegram bots, Discord, Slack and GitHub from this phone; pair someone to a bot or take them out.
- WhatsApp has its own row in Extensions, and the linked number stays reachable from computer management.
- A workflow link can name the model and its options for the target bot's turn.
- Every chat can have threads: open one from any message or the list; replies stay in the thread and the chat shows a "3 replies" chip.
- Voice calls add ElevenLabs: on your own key, Scribe hears you and the account's own voices read replies; Test says "OK".
- Apply Hermes environment, memory and delegation routes together.
- Local NeuTTS streams replies; talk to interrupt. Optional foreground Hey Hermes.
- Extensions adds offline Hermes skills, MCP apps and restorable skill improvements.
- Voice notes reuse speech keys; WhatsApp delivers routine reports.
- Marketplace agents include skills and routines; Workflow Builder links steps.
- Browser profiles, chat colors and synced deletion. Event workflows and TickTick.

## 2.32.0

### zh-Hant
- Bot 清單可以分成自己的分組，例如「工作」和「個人」：長按 Bot 選「移到分組」，點分組標題即可收合，收合時會顯示裡面的未讀數。
- 遠端畫面連線更快，網路短暫中斷時也不會斷掉。

### en-US
- Sort the bot list into your own sections, like Work and Personal: touch and hold a bot and choose Move to section, then tap a section's heading to fold it, with its unread count showing while it's folded.
- The remote screen connects faster and stays up through a brief connection drop.
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
