<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.48.1
### zh-Hant
- Bot 會把較久的工作交給背景進行，聊天不會被占住：輸入框上方顯示進度，點一下可查看或停止；需要核准時卡片會出現在聊天裡，完成後 Bot 會告訴你結果。
- 從任何 App 的分享選單選「Codync」，就能把文字、連結、照片或檔案傳給 Bot。
- 智慧體可以有多個「登入設定」（賬戶或 API 金鑰）：bot 在「智慧體」選用哪一個，備援可選另一個，「用量上限」分別列出每個賬戶的額度。
- Bot 可以設定跟進，晚點自己回來告訴你結果；例行任務會從上次做到的地方接著做。
- 可以只回覆訊息的其中一段：長按訊息選「選取文字」，選好後點「回覆」。
- 開始 OpenAI 即時語音通話或遠端螢幕時，App 不會再直接關閉。
- 選擇卡片改用編號選項，也能直接打字回答。

### en-US
- Bots work in the background and keep chatting; a capsule above the message box shows or stops the work.
- Share text, links, photos or files to a bot from any app.
- Agents can have several login profiles (account or API key), with fallbacks and per-account limits.
- Bots can check back later; routines resume where they stopped.
- Reply to one part of a message.
- Starting an OpenAI realtime call or the remote screen no longer closes the app.
- Choice cards take your own answer.
