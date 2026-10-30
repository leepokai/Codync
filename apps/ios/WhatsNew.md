<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.33.2
### zh-Hant
- 可上傳或貼上網址設定 bot 與 agent 圖示，支援原色、單色與離線快取；自訂圖片維持靜態。
- 自訂圖片需要登入帳號；登入載入時只顯示進度。電腦選單新增裝置圖示。
- 選擇工作資料夾時可直接使用最近紀錄、貼上路徑，並在底部確認選擇。
- 即時動態與動態島的設定頁會說明兩者同步開啟或關閉。
- 電腦的推送通知設定會顯示系統權限狀態；未允許通知時，可直接前往開啟權限。
- 語音輸入尚未設定 API key 時，會直接開啟語音設定。

### en-US
- Upload images or paste URLs for bot and agent icons, with original colors, tinting and offline caching. Custom images stay still.
- Custom images require sign-in. Account loading shows progress, and computer menus include device icons.
- Choose workspaces from recent folders, paste a path, and confirm your selection at the bottom.
- Live Activity and Dynamic Island settings explain that both turn on and off together.
- Computer push settings show when notification permission is missing and help you enable it.
- Dictation opens voice settings when your provider API key is missing.
