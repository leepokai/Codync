<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.34.0
### zh-Hant
- 可以配對多台電腦：Bots 每台電腦一個標題，頂端選單可篩選要顯示的電腦。
- Bot 詳情新增「記憶」頁。
- 訊息框加入聽寫：裝置上辨識，或用你自己的 OpenAI／Gemini 金鑰轉錄。
- 新對話交接卡片可以保留原對話或編輯交接內容。
- 通話以來電樣式通知，可直接掛斷或靜音；新增 Claude Code、Codex 用量小工具。
- 設定加入「在 Google Play 評分」與「回報問題」；有新版本時會提醒。
- 長按 App 圖示選「Talk to Codync」直接和 lead bot 通話；從其他 App 分享文字可送到 bot 的訊息框。
- 掃描配對 QR code 更容易對焦。
- 自訂圖示現在只用在 bot 上；只用 Cursor CLI 登入時也能看到 Cursor 用量。

### en-US
- Pair several computers, each with a heading and filter in Bots.
- Bot details gain a Memory page; keep or edit a new chat's handoff.
- Dictate on device or with your own OpenAI or Gemini key.
- Calls show as Android calls; new Claude Code and Codex usage widgets.
- Talk to Codync, an icon shortcut, calls your lead; share text to a bot.
- Rate on Google Play, Report an issue, update notes; easier QR focus.
- Custom icons are for bots only; Cursor usage shows with a CLI sign-in.
