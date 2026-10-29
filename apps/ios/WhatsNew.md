<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.31.2

### zh-Hant
- 從手機更新電腦時，電腦重新啟動的期間會顯示「正在更新…」，不會再要你申請訪問。
- Codex bot 現在拿得到和你 Codex 設定裡同名的聯結器（例如 Notion），不會再看不到它的工具。
- Codex bot 傳訊息和整理記憶的動作，不會再出現在工作步驟裡。

### en-US
- While a computer restarts for an update you started from your phone, it shows Updating… instead of asking you for access.
- Codex bots now get a connector that shares its name with a server in your Codex config, such as Notion, instead of missing its tools.
- A Codex bot's own messages and memory upkeep no longer show among its working steps.
