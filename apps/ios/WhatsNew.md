<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.46.1
### zh-Hant
- bot 詳細資料的「電腦」分頁改成點一下才開始顯示畫面，不會一打開就連線。
- 遠端畫面在 5 分鐘沒有碰 App 後會自動暫停以節省流量，點一下就能繼續，原本的「接手控制」或「交還給 Bot」都會保留。
- 權限核准卡片與連線請求卡片現在會跟著 app 的語言顯示。

### en-US
- The Computer tab in bot details starts its picture when you tap it, instead of connecting as soon as it opens.
- A computer's screen pauses after 5 minutes without you touching the app, to save data. Tap to continue; your Take over or Hand back to bots choice stays.
- Approval and connection request cards now follow the app's language.
