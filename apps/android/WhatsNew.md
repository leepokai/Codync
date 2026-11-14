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
- 從任何 App 的分享選單選「Codync」，就能把文字、連結、照片或檔案傳給 Bot，還能附上一段訊息。上次分享的對象會排在「最近」並預先選好，原本輸入框裡的草稿不會被動到。
- 更新提示會清楚指出需要更新這個 App，還是電腦上的 Codync；雲端電腦更新時也會顯示進度說明。
- 在語音訊息載入時離開聊天，該語音不會在離開後才開始播放。
- 選擇卡片改用編號選項；最後一列「其他答案」可以直接打字回答，bot 會帶著你的話繼續。
- 智慧體可以有多個「登入設定」：新增賬戶（Claude Code、Codex 等），或為任何智慧體新增 API 金鑰；每個都能取名，bot 在「智慧體」選擇要用哪一個。
- 備援可以選同一個智慧體的另一個登入設定。
- Bot 說「晚點回來看」時會設定跟進，時間到了自己回到聊天告訴你結果。
- 例行任務會記住上次做到哪裡，下次接著做。

### en-US
- Share text, links, photos or files to a bot from any app's share sheet.
- Update notices say whether this app or your computer's Codync needs updating.
- A voice note no longer plays after you leave its chat.
- Choice cards show numbered options and take your own answer.
- Agents can have several login profiles (an account or an API key): name each, pick a bot's under Agent, or fall back to another.
- Bots can check back later with a follow-up.
- Routines pick up where the last run stopped.
