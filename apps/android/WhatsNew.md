<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.28.0

### zh-Hant
- 電腦連成網路後，Bot 可以請其他電腦上的 Bot 幫忙。
- 可以從 ChatGPT 或 Claude 透過聯結器請你的 Bot 做事。

### en-US
- Bots can ask bots on your other computers once they share a network.
- ChatGPT or Claude can give your bots tasks through the Codync connector.

## 2.27.0

### zh-Hant
- 擴充套件的列表改回往下捲動，聯結器捲到底會自動載入更多。點項目只看詳情，按「Add」才加入。
- 用量也會顯示這台電腦上其他 agent 的額度。Cursor 會用本機登入向 cursor.com 查詢。

### en-US
- Extensions lists scroll again; connectors load more at the end. Tapping one only opens it; Add adds it.
- Adds other agents' limits. Cursor asks cursor.com.

## 2.26.2

### zh-Hant
- Bot 傳來的圖片會直接顯示在聊天裡，點一下可以放大看，不用先下載。
- 電腦上的 Codync 版本太舊時，更新提示多了「Update now」按鈕，直接從手機叫那台電腦更新。
- 擴充套件一次顯示一個分類：智慧體、聯結器或技能，每個列表都分頁。點列表兩側的圓形按鈕或左右滑動翻頁，每頁可選 6、10、20 或 50 筆。
- 電腦使用自建中繼時，設定和連線狀態會顯示該中繼的地址，不再一律寫成 Cloudflare。
- 可以建立使用 WorkBuddy 的 Bot，在聊天裡登入即可。

### en-US
- Images a bot sends now show right in the chat; tap one to see it full size, no download needed.
- When a computer's Codync is too old for this app, Update now on the notice updates that computer from your phone.
- Extensions shows Agents, Connectors or Skills in pages: tap the round buttons or swipe, 6 to 50 per page.
- A private relay shows its address in Settings and status.
- Bots can run WorkBuddy; sign in from the chat.

## 2.26.0

### zh-Hant
- Bot 工作時，Gemini 與 OpenAI 的語音會主動告訴你它在做什麼、有沒有等你核准或遇到問題，做完就念出結果。
- 通話中短暫斷線再連上時，錯過的回覆會補念給你聽。
- Gemini 或 OpenAI 通話斷線時會自動重新連上並接著聊，不必重打。
- 通話的通知會顯示通話時長、狀態和 Bot 正在做的事。
- Gemini 與 OpenAI 的語音用你正在說的語言回答，你換語言它就跟著換。

### en-US
- While the bot works, the Gemini and OpenAI voice says what it's doing, when an approval waits or something fails, and reads the result.
- Replies missed while a call reconnects are read once it's back.
- A dropped Gemini or OpenAI call reconnects without hanging up.
- The call's notification shows its length, state and what the bot is doing.
- The Gemini and OpenAI voice answers in the language you speak and follows when you switch.

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
