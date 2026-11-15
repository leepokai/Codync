<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.49.0
### zh-Hant
- Bot 會把比較久的工作交給背景進行，而且可以同時做好幾件：聊天不會被占住，你可以繼續傳訊息，它也會照常回答。
- 輸入框上方會有一個「背景」膠囊，顯示幾件進行中、幾件等待中；只有一件在做時會顯示它的進度與時間，也可以直接停止（已經改的內容會留著）。點一下會打開 Bot 的「詳情」。
- 「詳情」現在列出 Bot 在做的事：「Threads」是正在進行的工作和還空著的名額，點一列可以看它做了什麼；「Event Box」是依序等待的工作，可以「先做」、名額滿時「插隊」（選一件讓位，它之後會重新開始），或長按「丟掉」。
- 「最近」列出一天內結束的工作：完成的可以點開看過程，停止或失敗的會說明原因，並且可以「再試一次」。
- 「同時幾個 agent」在 Bot 的「設定」裡，可以設 1 到 6 個。
- 「指令」和「智慧體」（模型、快速模式、備援等）從「詳情」移到 Bot 的「設定」最上方。
- 背景工作需要你核准時，卡片會出現在聊天裡並標示「背景」，照平常的方式回答就好。
- 背景工作完成後，Bot 會用一則訊息告訴你結果；沒有完成也會說明原因。
- 例行任務和其他 Bot 的請求現在也在背景進行，不再占住聊天：例行任務執行時，你可以繼續和 Bot 對話。
- Bot 列表上方新增「工作」按鈕（有背景工作時才會出現）：一次看到每個 Bot 正在進行和等待中的背景工作，操作和「詳情」裡一樣。
- 背景工作可以在聊天裡問你問題（選擇卡片）、傳檔案或語音給你，並標示「背景」和那件工作的名稱。
- 「每個 agent 一個 worktree」在 Bot 的「設定」裡（資料夾是 Git repository 時才會出現）：同時在同一個 repository 工作的 agent 各有自己的分支，不會互相改到檔案。Codync 不會合併或刪除有內容的分支。
- 背景工作還在進行時，「實時活動」會一直留著，需要你時也會顯示。
- 從任何 App 的分享選單選「Codync」，就能把文字、連結、照片或檔案傳給 Bot，還能附上一段訊息。上次分享的對象會排在「最近」並預先選好。
- 更新提示會清楚指出需要更新這個 App，還是電腦上的 Codync；雲端電腦更新時也會顯示進度說明。
- 智慧體可以有多個「登入設定」：在智慧體設定頁的「登入設定」新增賬戶（Claude Code、Codex、OpenCode、Pi、Google Antigravity 等可在同一台電腦登入多個賬戶），或為任何智慧體新增 API 金鑰（自行輸入環境變數，或從電腦的登入 shell 匯入；值只留在電腦上）。每個登入設定（包含預設的）都能自己取名，再到 bot 設定的「智慧體」選擇它要用的登入設定，兩個 bot 就能同時用不同的登入工作。
- 備援可以選同一個智慧體的另一個登入設定：一個的額度用完，bot 會換到下一個繼續。
- 「用量上限」會在智慧體底下分別列出每個賬戶的額度（Claude Code、Codex），小工具顯示最接近上限的那一個。
- 電腦重新啟動或 App、Codync 更新後，iPhone 只接收新的訊息，不再重新載入每個聊天。
- 選擇卡片改用編號選項；最後一列「其他答案」可以直接打字回答，bot 會帶著你的話繼續。
- Bot 說「晚點回來看」時會真的設定跟進：聊天裡會出現「20 分鐘後跟進」的提示，時間到了它會自己回到聊天告訴你結果，還沒有進展就不打擾你。等待中的跟進會列在例行任務上方，可以直接取消。
- 例行任務會記住上次做到哪裡，分批進行的工作下次會接著做。
- Gemini 即時語音新增 Google 搜尋開關，可在通話中查詢網頁資訊，預設關閉。
- 語音通話設定的 OpenAI 區塊會顯示電腦目前使用的「伺服器」：OpenAI，或你在電腦上設定的自有語音伺服器。使用自有伺服器時，模式固定為語音識別。
- 可以只回覆訊息的其中一段：長按訊息選「選取文字」，選好文字後在選單點「回覆」，Bot 會知道你問的是哪一段。
- Bot 設定的「技能」只列出名稱，點旁邊的資訊按鈕可以看完整說明。
- 裝置上聽寫與語音通話不會再弄丟停頓前說的話。
- Bot 改用備援時，「智慧體」卡片會說明原本的智慧體為什麼停下（已登出、被限速等），只有用量上限才顯示重設時間。
- 輸入 `/` 時，清單最後會列出 Bot 所用智慧體自己的指令（Claude Code、Codex、OpenCode 等），和 Codync 自己的指令、技能分開，標上智慧體的名稱、圖示和說明；選一個會填入 `/<指令>`，送出後由智慧體直接執行。

### en-US
- Bots do long work in the background, several things at once: the chat stays free, so you can keep writing and the bot keeps answering.
- One capsule above the message box says "In the background" with how many are working and waiting; with a single one it shows what it is doing and for how long, and you can stop it there (what it already changed stays). Tap it to open the bot's Details.
- Details now lists what the bot does: Threads shows the work that runs now and the places still free, and a row opens what it did; Event box shows what waits, in order, where you can move one First, Cut in when every place is taken (you pick what gives way; it starts again later), or touch and hold to Drop it.
- Recent lists the work that ended in the last day: open a finished one to see what it did; a stopped or failed one says why and has Try again.
- Agents at once, in the bot's Settings, sets how many work at the same time (1 to 6).
- Instructions and Agent (model, fast mode, fallbacks) moved from Details to the top of the bot's Settings.
- When background work needs your approval, its card appears in the chat marked "In the background"; answer it like any other.
- When the work is done, the bot tells you what it found in a message, or why it couldn't finish.
- Routines and other bots' requests now run in the background too and no longer hold the chat: you can keep talking to the bot while a routine runs.
- A Work button at the top of the bot list (there while any bot has background work) shows every bot's work that is running or waiting, with the same actions as in Details.
- Background work can ask you a question in the chat (a choice card) and send you a file or a voice note there, marked "In the background" with the name of the work.
- Worktree per agent, in the bot's Settings (shown when its folder is a Git repository): agents working at once in one repository each get their own branch, so they don't change the same files. Codync never merges or deletes a branch that has work on it.
- The Live Activity stays while background work runs, and shows when it needs you.
- Share text, links, photos or files to a bot from any app: choose Codync in the share sheet and add a message if you like. The bots you shared to last are under Recent, with the latest one already picked.
- Update notices clearly identify whether this app or Codync on your computer needs updating, and explain when a cloud computer is updating itself.
- Agents can have more than one login profile: under Login profiles in an agent's setup, add an account (Claude Code, Codex, OpenCode, Pi, Google Antigravity and more can be signed in to several on one computer), or add an API key for any agent (environment variables you type or import from the computer's login shell; their values stay on the computer). Name any login profile (the default one too), then pick a bot's under Agent in its settings, so two bots can work at once on different logins.
- A fallback can be the same agent on another of its login profiles: when one runs out of usage, the bot carries on with the next.
- Usage limits lists each account of an agent with its own limits (Claude Code, Codex), and widgets show the one nearest its limit.
- After your computer restarts or the app or Codync updates, the iPhone catches up on only what's new instead of reloading every chat.
- Choice cards show numbered options, and the last row, Something else, takes your own answer; the bot continues with your words.
- A bot that says it will check back later now sets a follow-up: a line such as "Follow-up in 20 minutes" shows in the chat, and when it's due the bot returns on its own to tell you what it found, or stays quiet if there's nothing yet. Waiting follow-ups are listed above the bot's routines, where you can cancel one.
- A routine remembers where its last run stopped, so work done in parts carries on from there.
- Optional Google Search in Gemini realtime calls, off by default.
- Voice chat settings show which Server the computer's OpenAI engine uses: OpenAI, or a speech server of your own set on the computer. With your own server, Mode stays on Speech.
- Reply to one part of a message: touch and hold it, choose Select Text, select the words, then Reply. The bot is told which part you mean.
- A bot's Skills settings list only each skill's name; the info button beside one shows its description.
- On-device dictation and calls keep the words you said before a pause.
- While a bot is on a fallback, the Agent card says why its own agent stopped (signed out, rate limited and so on) and gives a reset time only for a usage limit.
- Typing `/` now also lists the bot's agent's own commands (Claude Code, Codex, OpenCode and more), listed apart from Codync's own commands and skills under the agent's name, with its logo and description; pick one to fill in `/<command>`, and the agent runs it when you send.
