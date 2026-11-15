<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.48.2
### zh-Hant
- Bot 把比較久的工作放到背景做，聊天照常回你。
- 點輸入框上方的膠囊開啟 Bot 的「詳情」：Threads（正在跑的）和 Event Box（等待中的：先做、插隊、丟掉）。
- 背景工作要核准時，卡片出現在聊天裡。
- 「同時幾個 agent」在 Bot 的「設定」。
- 從任何 App 的分享選單把文字、連結、照片或檔案傳給 Bot。
- Bot 可以設定跟進，晚點自己回來；例行任務接著上次做。
- 可以只回覆訊息的其中一段：長按訊息選「選取文字」。
- 開始 OpenAI 即時語音通話或遠端螢幕時，App 不會再關閉。
- 選擇卡片可以直接打字回答。
- 改用備援時，智慧體卡片會說明原因，只有用量上限才顯示重設時間。

### en-US
- Bots do long work in the background and keep answering.
- The capsule over the message box opens Details: Threads and Event box.
- Its approval cards appear in the chat.
- Agents at once is in the bot's Settings.
- Share text, links, photos or files to a bot from any app.
- Bots can check back later; routines resume.
- Reply to one part of a message.
- OpenAI calls and remote screen no longer close the app.
- Choice cards take your own answer.
- On a fallback, the Agent card says why.
