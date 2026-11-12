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

### en-US
- Bot details have a new Computer tab: the computer the bot runs on, its connection, and a small live picture of its screen, so you can watch the bot work. Tap the picture to open the full screen and take over. The Use the computer switch moved here.
- A cloud computer that's asleep wakes when you look, and the picture shows "Waking the computer…" until it's ready.
- Fixed a cloud computer's desktop freezing when its screen was open in more than one place at once.
