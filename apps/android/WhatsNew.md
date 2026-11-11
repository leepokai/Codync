<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.39.0

### zh-Hant
- Hermes 環境、記憶與子代理路由可一次套用。
- 通話可選 NeuTTS 本機串流語音，說話可中斷回覆；手機新增自選 Hey Hermes 前景喚醒。
- 擴充功能加入 Hermes 技能與 MCP 應用程式，標示來源；技能可離線安裝。
- Bot 可保留自己的技能改進，並依要求查看、還原版本。
- Telegram、WhatsApp、Slack、Discord 語音訊息沿用語音金鑰；例行工作可透過已連結的 WhatsApp 傳送報告。
- 市集的代理附技能與例行工作；Workflow Builder 可建立連動步驟。
- Bot 可使用獨立瀏覽器設定檔與聊天顏色；訊息刪除會同步。
- Slack、Telegram、Discord 與 GitHub 事件可啟動流程；新增 TickTick 等應用程式及代理資訊。

### en-US
- Apply Hermes environment, memory and delegation routes together.
- Local NeuTTS streams replies; talk to interrupt. Optional foreground Hey Hermes.
- Extensions adds offline Hermes skills, MCP apps and restorable skill improvements.
- Voice notes reuse speech keys; WhatsApp delivers routine reports.
- Marketplace agents include skills and routines; Workflow Builder links steps.
- Browser profiles, chat colors and synced deletion. Event workflows and TickTick.

## 2.32.0

### zh-Hant
- Bot 清單可以分成自己的分組，例如「工作」和「個人」：長按 Bot 選「移到分組」，點分組標題即可收合，收合時會顯示裡面的未讀數。
- 遠端畫面連線更快，網路短暫中斷時也不會斷掉。

### en-US
- Sort the bot list into your own sections, like Work and Personal: touch and hold a bot and choose Move to section, then tap a section's heading to fold it, with its unread count showing while it's folded.
- The remote screen connects faster and stays up through a brief connection drop.
