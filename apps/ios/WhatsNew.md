<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.34.0
### zh-Hant
- 「權限」改成列出代理程式自己的模式，例如 Claude Code 的 Auto、Manual、Plan。新 bot 預設 Auto：安全的事代理程式直接做，其餘的才問你。
- 自訂圖示現在只用在 bot 上，代理程式一律顯示自己的標誌。
- 只用 Cursor CLI 登入時，也能看到 Cursor 的用量。

### en-US
- Permissions now list your agent's own modes, like Claude Code's Auto, Manual and Plan. New bots start in Auto: the agent does what's safe and asks you about the rest.
- Custom icons are now for bots only; agents always show their own logo.
- Cursor usage now shows when you sign in with the Cursor CLI only.
