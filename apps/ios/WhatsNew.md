<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.46.2
### zh-Hant
- 更新提示會清楚指出需要更新這個 App，還是電腦上的 Codync；雲端電腦更新時也會顯示進度說明。
- 電腦重新啟動或 App、Codync 更新後，iPhone 只接收新的訊息，不再重新載入每個聊天。

### en-US
- Update notices clearly identify whether this app or Codync on your computer needs updating, and explain when a cloud computer is updating itself.
- After your computer restarts or the app or Codync updates, the iPhone catches up on only what's new instead of reloading every chat.
