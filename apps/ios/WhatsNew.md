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
- Claude Code、Codex、OpenCode、Pi、Google Antigravity 等智慧體可以在同一台電腦登入多個賬戶：在智慧體設定頁的「賬戶」新增，每個賬戶（包含預設賬戶）都能自己取名，再到 bot 設定的「智慧體」選擇它要用的賬戶，兩個 bot 就能同時用不同賬戶工作。
- 備援可以選同一個智慧體的另一個賬戶：一個賬戶的額度用完，bot 會換到下一個賬戶繼續。
- 電腦重新啟動或 App、Codync 更新後，iPhone 只接收新的訊息，不再重新載入每個聊天。
- 選擇卡片改用編號選項；最後一列「其他答案」可以直接打字回答，bot 會帶著你的話繼續。

### en-US
- Share text, links, photos or files to a bot from any app: choose Codync in the share sheet and add a message if you like. The bots you shared to last are under Recent, with the latest one already picked.
- Update notices clearly identify whether this app or Codync on your computer needs updating, and explain when a cloud computer is updating itself.
- Claude Code, Codex, OpenCode, Pi, Google Antigravity and more can be signed in to more than one account on a computer: add one under Accounts in the agent's setup, name any account (the default one too), then pick a bot's account under Agent in its settings, so two bots can work at once on different accounts.
- A fallback can be the same agent on another of its accounts: when one account runs out of usage, the bot carries on with the next.
- After your computer restarts or the app or Codync updates, the iPhone catches up on only what's new instead of reloading every chat.
- Choice cards show numbered options, and the last row, Something else, takes your own answer; the bot continues with your words.
