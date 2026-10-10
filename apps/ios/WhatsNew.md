<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice. tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.13.2

### zh-Hant

- App 或電腦更新後，聊天會重新載入最新訊息，不會再留下往上滑也補不回來的空缺。
- 討論串上方的回覆數改成和訊息下方的討論串標籤一致，只算訊息，不算 bot 的工具步驟。
- 讀過主聊天後，還沒打開的討論串（例如例行任務的執行過程）會保持未讀。
- 按下停止後，卡住的 bot 也會在幾秒內停下來。
- 記憶暫時無法使用時，bot 仍會回覆，並提醒一次。

### en-US

- After the app or your computer updates, a chat reloads its newest messages instead of leaving a gap that scrolling up never fills.
- A thread's reply count now matches its chip under the message: messages only, not the bot's tool steps.
- Reading a chat leaves threads you haven't opened, such as routine runs, unread.
- Stop now ends a stuck bot within seconds.
- When its memory is unavailable, a bot still answers and tells you once.
