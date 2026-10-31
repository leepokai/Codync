<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.34.0
### zh-Hant
- 自訂圖示現在只用在 bot 上，代理程式一律顯示自己的標誌。
- 只用 Cursor CLI 登入時，也能看到 Cursor 的用量。

### en-US
- Custom icons are now for bots only; agents always show their own logo.
- Cursor usage now shows when you sign in with the Cursor CLI only.
