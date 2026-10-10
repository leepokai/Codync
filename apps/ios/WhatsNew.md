<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice. tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.13.1

### zh-Hant

- 底部分頁改用 iOS 原生的分頁列。
- 在對話頂端往下拉，圓形箭頭會跟著畫出來，拉滿放開即可重新連線電腦；沒拉滿就放開不會觸發。
- 修正較短的對話頂端一直顯示載入圈圈的問題。

### en-US

- The tab bar at the bottom is now the native iOS tab bar.
- Pull down at the top of a chat to draw a circular arrow; let go once it's complete to reconnect to your computer. A shorter pull does nothing.
- Short chats no longer show a loading spinner at the top that never goes away.
