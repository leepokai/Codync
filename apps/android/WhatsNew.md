<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

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
