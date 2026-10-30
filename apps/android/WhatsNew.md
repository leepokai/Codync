<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.33.2
### zh-Hant
- 選擇工作資料夾時可使用最近紀錄、貼上路徑，並在底部確認。
- 上傳圖片或貼上網址設定 bot 與 agent 圖示，支援原色、單色及離線快取；自訂圖片維持靜態。
- 自訂圖片需要登入。登入載入時只顯示進度，電腦選單新增裝置圖示。
- 推送設定顯示通知權限，並協助前往開啟。

### en-US
- Choose recent workspaces, paste a path, and confirm at the bottom.
- Upload images or paste URLs for bot and agent icons, with colors, tinting and offline caching. Custom images stay still.
- Custom images require sign-in. Account loading shows progress; computer menus show device icons.
- Push settings show missing notification permission and help you enable it.
