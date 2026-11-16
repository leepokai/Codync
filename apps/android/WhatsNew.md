<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.49.0
### zh-Hant
- Bot 把比較久的工作放到背景做，聊天照常回你。
- 例行任務和其他 Bot 的請求也在背景進行，不再占住聊天。
- Bot 列表的「工作」按鈕列出每個 Bot 的背景工作。
- 背景工作要核准、要問你問題或傳檔案時，會出現在聊天裡。
- 點輸入框上方的膠囊開啟 Bot 的「詳情」：Threads（正在跑的）和 Event Box（等待中的：先做、插隊、丟掉）。
- 「同時幾個 agent」在 Bot 的「設定」。
- 「每個 agent 一個 worktree」：同時在同一個 repository 工作的 agent 各有自己的分支。
- 改用備援時，智慧體卡片會用 App 的語言說明原因。
- 備援的 ⋯ 選單新增「編輯」，順序不變。
- 輸入 `/` 時會列出 Bot 所用智慧體自己的指令。
- 網路規則新增「自動」與「詢問」等級，沒有符合規則的電腦預設為「自動」。
- 聽寫中按「傳送」會等說的話轉成文字再一起送出。
- 等待核准的卡片會算在 Bot 的未讀數字裡（琥珀色），直到你回覆為止。
- 長按 Bot 選「新會話」，讓智慧體從新的上下文開始，聊天記錄會保留。

### en-US
- Bots, routines and bot requests work in the background.
- The Work button lists background work.
- Approval cards stay in the badge, amber, until answered.
- Background work can ask or send files.
- Details: Threads, Event box.
- Settings: Agents at once, Worktree per agent.
- On a fallback, the Agent card says why.
- Fallbacks have Edit.
- `/` lists the agent's commands.
- Network rules gain Auto and Ask.
- Send while dictating keeps your words.
- Hold a bot for New session: a fresh context.
