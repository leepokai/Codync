<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.46.0
### zh-Hant
- bot 詳細資料新增「電腦」分頁：顯示 bot 所在的電腦和連線狀態，以及那臺電腦螢幕的即時小畫面，可以看著 bot 操作；點一下畫面就開啟完整螢幕，需要時可以接手。「使用電腦」開關也移到這個分頁。
- 睡著的雲電腦在你查看時會自動喚醒，畫面上會顯示「正在喚醒電腦…」直到準備好。
- 修正雲端電腦的螢幕同時被多處開啟時，整個桌面可能凍結的問題。
- 修正電腦螢幕重新連線或從背景回到 app 後，「接手控制」或「交還給 Bot」的選擇沒有保留的問題。

### en-US
- New Computer tab in bot details: the bot's computer, its connection, and a live picture of its screen to watch the bot work. Tap it for the full screen. Use the computer moved here.
- A sleeping cloud computer wakes when you look.
- Fixed a cloud computer's desktop freezing when its screen was open in several places.
- Take over or Hand back to bots stays as you left it after a reconnect or coming back to the app.
