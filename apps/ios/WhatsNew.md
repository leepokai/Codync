<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.49.1
### zh-Hant
- Bot 會把比較久的工作交給背景進行，而且可以同時做好幾件：聊天不會被占住，你可以繼續傳訊息，它也會照常回答。
- 輸入框上方會有一個「背景」膠囊，顯示幾件進行中、幾件等待中；只有一件在做時會顯示它的進度與時間，也可以直接停止（已經改的內容會留著）。點一下會打開 Bot 的「詳情」。
- 「詳情」現在列出 Bot 在做的事：「Threads」是正在進行的工作和還空著的名額，點一列會打開那個 thread 的頁面，看它做了什麼，也可以像聊天一樣直接傳訊息給它：它還在做時訊息會先排隊，可以「現在送」或「移除」；已經結束的 thread 收到訊息會再繼續。每個 thread 會顯示它工作的資料夾或分支；「Event Box」是依序等待的工作，可以「先做」、名額滿時「插隊」（選一件讓位，它之後會重新開始），或長按「丟掉」。
- 「最近」列出一天內結束的工作：完成的可以點開看過程，停止或失敗的會說明原因，並且可以「再試一次」。
- 「同時幾個 agent」在 Bot 的「設定」裡，可以設 1 到 6 個。
- 「設定」的「工作區」會說明資料夾是不是 Git repository，旁邊的資訊按鈕會打開說明文件。
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
- 語音通話設定的 OpenAI 區塊會顯示電腦目前使用的「伺服器」：OpenAI，或你在電腦上設定的自有語音伺服器。使用自有伺服器時，模式固定為語音識別。
- 可以只回覆訊息的其中一段：長按訊息選「選取文字」，選好文字後在選單點「回覆」，Bot 會知道你問的是哪一段。
- Bot 也可以引用訊息：晚一點才回的答案，上方會顯示它回答的是哪一則。點一下引用就會捲到那則訊息。
- Bot 設定的「技能」只列出名稱，點旁邊的資訊按鈕可以看完整說明。
- 裝置上聽寫與語音通話不會再弄丟停頓前說的話。
- Bot 改用備援時，「智慧體」卡片會說明原本的智慧體為什麼停下（已登出、被限速等），只有用量上限才顯示重設時間；這一行和備援底下的休息時間都會跟著 App 的語言。
- 輸入 `/` 時，清單最後會列出 Bot 所用智慧體自己的指令（Claude Code、Codex、OpenCode 等），和 Codync 自己的指令、技能分開，標上智慧體的名稱、圖示和說明；選一個就會填入那個指令，送出後由智慧體直接執行。
- 電腦的「網路」頁會列出新的規則等級：「自動」只讓其他電腦的 Bot 做 agent 判斷為安全的事，其餘先問你；「詢問」讓每個變更和指令都等你批准。沒有符合任何規則的電腦預設為「自動」，原本就在網路裡的電腦維持「允許」。
- 備援直接在「智慧體」卡片裡設定，和 Bot 自己的智慧體一樣：點一列就能換它的智慧體、模型或登入設定，順序不變；按「+」新增的備援會直接展開。
- 聽寫中直接按「傳送」，會等說的話轉成文字，再和打好的字一起送出，不會再丟掉。
- 等待核准的卡片會一直算在 Bot 的未讀數字裡（顯示為琥珀色），打開聊天或「標示為已讀」都不會讓它消失，直到你回覆為止；收起的分組裡有 Bot 在等你時，分組的數字也會變成琥珀色。
- 長按 Bot 選「新會話」，就能讓智慧體從新的上下文開始，聊天記錄會保留。
- 背景工作也會用備援；需要登入時會在聊天裡請你登入。
- 轉寫中、資料夾、雲端電腦更新等幾段介面文字，現在會以你選的語言顯示。

### en-US
- Bots do long work in the background, several things at once, and keep answering in the chat.
- A capsule above the message box says how many are working and waiting (one alone: what it's doing, with Stop). Tap it for the bot's Details.
- Details lists the bot's work. Threads: what runs now; tap one to write to it like a chat, even once it's done. Each shows its folder or branch. Event box: what waits, with First, Cut in and Drop. Recent: what ended, with Try again.
- Agents at once, in the bot's Settings, sets how many work at a time (1 to 6). Instructions and Agent (model, fast mode, fallbacks) moved there from Details.
- Background work can ask for approval or a choice and send files or voice notes in the chat, marked "In the background".
- Routines and other bots' requests run in the background too.
- Settings > Workspace says if the folder is a git repository.
- A Work button at the top of the bot list shows every bot's running or waiting work.
- Worktree per agent, in the bot's Settings (for a Git repository): agents working at once each get their own branch.
- The Live Activity stays while background work runs, and shows when it needs you.
- Share text, links, photos or files to a bot from any app: choose Codync in the share sheet, with a message if you like. Bots you shared to last are under Recent.
- Update notices say whether this app or Codync on your computer needs updating, and when a cloud computer is updating itself.
- Agents can have several login profiles: add an account or an API key in an agent's setup (keys stay on the computer) and pick a bot's under Agent.
- A fallback can be the same agent on another login profile: when one runs out of usage, the bot carries on with the next.
- Usage limits lists each account of an agent with its own limits (Claude Code, Codex), and widgets show the one nearest its limit.
- After a restart or an update, the iPhone loads only what's new, not every chat again.
- Choice cards show numbered options; the last row, Something else, takes your own answer.
- A bot that says it will check back later sets a follow-up ("Follow-up in 20 minutes") and tells you what it found when it's due. Waiting ones are above its routines.
- A routine remembers where its last run stopped, so work done in parts carries on from there.
- Voice chat settings show which Server the computer's OpenAI engine uses: OpenAI, or your own speech server (Mode then stays on Speech).
- Reply to one part of a message: touch and hold it, choose Select Text, select the words, then Reply.
- Bots can quote a message too: tap the quote above an answer to see it.
- A bot's Skills settings list only each skill's name; the info button beside one shows its description.
- On-device dictation and calls keep the words you said before a pause.
- On a fallback, the Agent card says why the bot's agent stopped (signed out, rate limited, etc.).
- Typing `/` also lists the agent's own commands (Claude Code, Codex, OpenCode and more) under its name; pick one and the agent runs it when you send.
- Network rules gain two levels: Auto (another computer's bots get only what the agent judges safe; the rest asks you) and Ask (everything waits for you). A computer no rule matches gets Auto.
- Fallbacks are set right in the Agent card, like the bot's own agent: tap one to change its agent, model or login profile in place; + adds one already open.
- Send while dictating waits for what you said and sends it with what you typed.
- A waiting approval card stays in the bot's badge, in amber, until you answer it.
- Touch and hold a bot for New session: a fresh context for its agent; the chat stays.
- Background work uses fallbacks too; sign-ins it needs show in the chat.
- A few more interface texts (transcribing, folders, cloud computer updates) now show in your language.
