<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.47.0
### zh-Hant
- 從任何 App 的分享選單選「Codync」，就能把文字、連結、照片或檔案傳給 Bot，還能附上一段訊息。上次分享的對象會排在「最近」並預先選好。
- 更新提示會清楚指出需要更新這個 App，還是電腦上的 Codync；雲端電腦更新時也會顯示進度說明。
- 智慧體可以有多個「登入設定」：在智慧體設定頁的「登入設定」新增賬戶（Claude Code、Codex、OpenCode、Pi、Google Antigravity 等可在同一台電腦登入多個賬戶），或為任何智慧體新增 API 金鑰（自行輸入環境變數，或從電腦的登入 shell 匯入；值只留在電腦上）。每個登入設定（包含預設的）都能自己取名，再到 bot 設定的「智慧體」選擇它要用的登入設定，兩個 bot 就能同時用不同的登入工作。
- 備援可以選同一個智慧體的另一個登入設定：一個的額度用完，bot 會換到下一個繼續。
- 「用量上限」會在智慧體底下分別列出每個賬戶的額度（Claude Code、Codex），小工具顯示最接近上限的那一個。
- 電腦重新啟動或 App、Codync 更新後，iPhone 只接收新的訊息，不再重新載入每個聊天。
- 選擇卡片改用編號選項；最後一列「其他答案」可以直接打字回答，bot 會帶著你的話繼續。
- Bot 說「晚點回來看」時會真的設定跟進：聊天裡會出現「20 分鐘後跟進」的提示，時間到了它會自己回到聊天告訴你結果，還沒有進展就不打擾你。
- 例行任務會記住上次做到哪裡，分批進行的工作下次會接著做。

### en-US
- Share text, links, photos or files to a bot from any app: choose Codync in the share sheet and add a message if you like. The bots you shared to last are under Recent, with the latest one already picked.
- Update notices clearly identify whether this app or Codync on your computer needs updating, and explain when a cloud computer is updating itself.
- Agents can have more than one login profile: under Login profiles in an agent's setup, add an account (Claude Code, Codex, OpenCode, Pi, Google Antigravity and more can be signed in to several on one computer), or add an API key for any agent (environment variables you type or import from the computer's login shell; their values stay on the computer). Name any login profile (the default one too), then pick a bot's under Agent in its settings, so two bots can work at once on different logins.
- A fallback can be the same agent on another of its login profiles: when one runs out of usage, the bot carries on with the next.
- Usage limits lists each account of an agent with its own limits (Claude Code, Codex), and widgets show the one nearest its limit.
- After your computer restarts or the app or Codync updates, the iPhone catches up on only what's new instead of reloading every chat.
- Choice cards show numbered options, and the last row, Something else, takes your own answer; the bot continues with your words.
- A bot that says it will check back later now sets a follow-up: a line such as "Follow-up in 20 minutes" shows in the chat, and when it's due the bot returns on its own to tell you what it found, or stays quiet if there's nothing yet.
- A routine remembers where its last run stopped, so work done in parts carries on from there.
