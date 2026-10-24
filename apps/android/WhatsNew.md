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
- 每台電腦都有一個主助理，固定在列表最上面：它會主動跟進你的事，每天兩次看看有沒有該提醒或能幫忙的，需要時請你授權 Gmail 等服務；打電話給它時由它先開口。
- 語音通話：連線中顯示來回的線，中途換語音不必掛斷，結束只留一行標題，點開看逐字稿。
- Bot 可以做 Artifact：報告、追蹤表、儀表板這類要回來看的內容會以卡片出現在聊天裡，點開看完整內容，直接在下方留言請它改，改好就自動更新。
- 每個 Bot 的「資料庫」收齊它做的 Artifact，以及聊天裡分享過的檔案和圖片。
- 可以建立使用 Hermes Agent 的 Bot。
- Bot 之間交辦工作不必等待，對方的回覆會回到發問的 Bot，再由它回覆你。
- 帳號選單裡的「電腦與設定」和它打開的頁面同名。
- 新增聯結器失敗後再試一次，會使用你改好的金鑰。
- 在設定開啟「顯示思考細節」後，點一下工作中 Bot 的狀態膠囊，就能展開這一輪的思考與步驟。
- 點對話上方的 Bot 會打開它的詳情、資料庫與設定；「新會話」改到 Bot 列表的長按選單。

### en-US
- Each computer has a lead bot on top that follows up, checks in twice a day and speaks first on calls.
- Calls leave one titled line; tap it for the transcript. Change voices mid-call as settings close.
- Bots make artifacts as chat cards you can ask to edit; each bot's Library holds them and its files.
- Create Hermes Agent bots; bots hand off work without waiting.
- Settings › Show thinking details unfolds a bot's steps.
- Tap a chat's title for bot details; New session is under touch & hold.
