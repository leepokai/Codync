<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.25.0

### zh-Hant
- Bot 可以做 Artifact：報告、追蹤表、儀表板這類要回來看的內容會以卡片出現在聊天裡，點開看完整內容，直接在下方留言請它改，改好就自動更新。
- 每個 Bot 的「資料庫」收齊它做的 Artifact，以及聊天裡分享過的檔案和圖片。
- 可以建立使用 Hermes Agent 的 Bot。
- Bot 之間交辦工作不必等待，對方的回覆會回到發問的 Bot，再由它回覆你。
- 帳號選單裡的「電腦與設定」和它打開的頁面同名。
- 新增聯結器失敗後再試一次，會使用你改好的金鑰。
- Bot 工作中點一下狀態膠囊，就能在對話裡展開這一輪的思考、工具與 MCP 呼叫，太長時在膠囊內捲動。
- 點對話上方的 Bot 會打開它的詳情、資料庫與設定；「新會話」改到 Bot 列表的長按選單。

### en-US
- Bots make artifacts, shown as cards in the chat; message the bot below one to edit it.
- Each bot's Library holds its artifacts and the chat's files.
- Create bots that run Hermes Agent.
- Bots hand work to each other without waiting.
- Computers & settings now matches the page it opens.
- Retrying a failed connector uses the corrected key.
- Tap a working bot's status pill to see this turn's steps.
- Tap a chat's title for the bot's details; New session moved to the list's touch & hold menu.
