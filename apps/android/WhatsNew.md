<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.47.0
### zh-Hant
- 在 bot 的選單裡可以「標為未讀」：未讀徽章和數量會回來，而且在每台裝置上都會保留，直到你重新打開聊天、標為已讀或在聊天裡傳送訊息。

### en-US
- Mark a bot's chat as unread from its menu: the unread badge and count come back and stay on every device until you open the chat again, mark it read or send from it.

## 2.46.2
### zh-Hant
- 更新提示會清楚指出需要更新這個 App，還是電腦上的 Codync；雲端電腦更新時也會顯示進度說明。
- 在語音訊息載入時離開聊天，該語音不會在離開後才開始播放。

### en-US
- Update notices clearly identify whether this app or Codync on your computer needs updating, and explain when a cloud computer is updating itself.
- Leaving a chat while a voice note is loading no longer lets it start playing afterward.

## 2.46.1
### zh-Hant
- bot 詳細資料的「電腦」分頁改成點一下才開始顯示畫面，不會一打開就連線。
- 遠端畫面在 5 分鐘沒有碰 App 後會自動暫停以節省流量，點一下就能繼續，原本的「接手控制」或「交還給 Bot」都會保留。
- 權限核准卡片與連線請求卡片現在會跟著 app 的語言顯示。

### en-US
- The Computer tab in bot details starts its picture when you tap it, instead of connecting as soon as it opens.
- A computer's screen pauses after 5 minutes without you touching the app, to save data. Tap to continue; your Take over or Hand back to bots choice stays.
- Approval and connection request cards now follow the app's language.
