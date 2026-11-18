<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.49.3
### zh-Hant
- Bot 預設的工作區改名為「專屬資料夾」。
- 電腦有安裝 Git 時，Codync 會在每一輪對話前後替 Bot 的專屬資料夾留一個版本：可以請 Bot 說明某一輪改了什麼，或把檔案還原。「設定」的「工作區」下方會顯示 Git repository。
- 在專屬資料夾裡，Bot 的背景工作都在同一個資料夾進行，所以那裡不再顯示「每個 thread 一個 worktree」開關。
- Bot 的背景工作現在一律叫 thread：「設定」裡改為「同時幾個 thread」和「每個 thread 一個 worktree」。
- 需要更新時，更新卡片上的「立即更新」「更新 Codync」按鈕會跟著 App 語言顯示。
- 讀完聊天後，它的通知會自動消失，桌面圖示上的標記也會跟著消失。
- Bot 在做事時旁邊的小圖示換成跟頭像同一種點陣，每種狀態各有自己的動作。
- 從睡眠中醒來的 Codync Cloud 雲端電腦會顯示「正在喚醒電腦…」，不再顯示離線。
- 傳出第五則訊息後，Codync 可能會邀請你在 Google Play 評分；之後至少相隔 120 天。

### en-US
- A bot's default workspace is now Own folder.
- With Git, Codync versions a bot's own folder each turn: ask what changed, or bring a file back.
- Background work is now called a thread: Threads at once, Worktree per thread.
- Update card buttons follow the app's language.
- Reading a chat clears its notification and badge.
- A busy bot's small icon uses the avatars' dot grid.
- A waking Codync Cloud computer shows Waking the computer…
- Codync may ask for a rating after your fifth message.

## 2.49.1
### zh-Hant
- Bot、例行任務和其他 Bot 的請求在背景工作，聊天照常回你。
- 「工作」按鈕列出每個 Bot 的背景工作。
- Bot 的「詳情」有 Threads 和 Event Box。
- 點一個 thread 會開成一頁，可以像聊天一樣傳訊息；等待中的訊息可以「現在送」或「移除」。
- 做完的 thread 也能再接著傳訊息。
- 每個 thread 會顯示它的資料夾或分支。
- 「設定」會說明資料夾是不是 git repository。
- 背景工作可以問你問題或傳檔案。
- 背景工作也會用備援；需要登入時會在聊天裡請你登入。
- 轉寫中、雲端電腦更新等幾段介面文字，現在會以你選的語言顯示。

### en-US
- Bots, routines and bot requests work in the background.
- The Work button lists background work.
- Details: Threads, Event box.
- Open a thread as a page and chat in it; Send now or Remove what waits.
- A finished thread takes messages again.
- Each thread shows its folder or branch.
- Settings says whether the folder is a git repository.
- Background work can ask or send files.
- Background work uses fallbacks; sign-ins show in the chat.
- More interface text shows in your language.
