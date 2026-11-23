<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.53.0
### zh-Hant
- Claude Code 的 Bot 使用 subagent 時，展開的工作步驟、「完整對話」和 task 的 thread 裡，subagent 做的步驟會縮排在啟動它的那一步底下；那一步也會顯示 subagent 的類型、用了多少 token 和花了多久，在背景執行的則顯示「在背景執行」。
- 第一次打開很長的聊天時，會直接停在最新的訊息，不會再跳到較早的訊息。

### en-US
- When a Claude Code bot uses subagents, each subagent's steps sit indented under the step that started it, in the unfolded working steps, Full conversation and a task's thread. That step also shows the subagent's type, the tokens it used and how long it took, or "in background" for one that runs in the background.
- Opening a long chat for the first time now lands on its newest message instead of earlier ones.

## 2.52.0
### zh-Hant
- 有些 agent（例如 Hermes）會在回覆中途問你問題，現在這些問題會顯示在聊天的選擇卡片上，等你回答，不會再被「自動核准」替你選第一個選項。有選項的問題只能選其中一個，卡片上不再顯示「其他答案」那一列。
- Bot 設定新增「顧問模式」：thread 可以用自己的 agent、模型和備用列表。
- 等你回覆的 thread 不再佔名額，下一件 task 可以先開始。
- 你補充正在進行的 task，Bot 會轉給它的 thread。
- 寫給 thread 的訊息也能附照片和檔案。

### en-US
- Some agents, such as Hermes, ask you questions mid-reply. These now wait for you on the chat's choice card, and Approve automatically no longer picks their first option for you. A question with options takes one of them, so its card has no Something else row.
- Advisor mode: a bot's threads can run on their own agent, model and fallbacks.
- A thread waiting for you frees its place for the next task.
- The bot passes what you add about a task on to its thread.
- Messages to a thread can carry photos and files.

## 2.51.0
### zh-Hant
- Bot 離開聊天做的事現在有兩個清楚的名稱：一件工作叫 task，Bot 做 task 的地方叫 thread，有自己的工作階段。一個 thread 一次做一件 task，做完可以接下一件。
- 聊天上方的「背景」那一行改名為「工作」，和「工作」頁同名。
- thread 都在忙、要選一件讓位時，提示改成「所有 thread 都在忙」；停止一件工作時問的是「停止這個 task？」。
- 配對說明改成「配對手機」，和電腦選單上的名稱一致：Mac 選單列原本的「配對 iPhone…」現在叫「配對手機…」。
- 第一次設定時可以直接建立 Codync Cloud 雲端電腦，不用先在電腦上安裝 Codync。

### en-US
- A bot's work away from the chat has two names: a task is one piece of work, a thread where the bot does tasks, one at a time, with its own session.
- The "In the background" line above the chat is now "Work".
- Prompts say "Every thread is busy" and "Stop this task?".
- Pairing instructions now say Pair phone, as the computer's menu does.
- First-time setup can start a Codync Cloud computer instead of installing Codync on yours.

## 2.50.0
### zh-Hant
- 建立 Codync Cloud 雲端電腦改成在同一個畫面一步一步來：先選供應商（看得到在哪裡執行、要多少錢），再照著步驟到供應商網站建立金鑰並貼上，最後取名、建立。
- 可以替自己的電腦取名：在「管理電腦」長按電腦的卡片，選「重新命名…」；留空就改回主機名稱。
- Bot 之間互相交接到上限（連續 8 次）而停下時，聊天裡會出現一則附「繼續」按鈕的提示：點一下就會替你傳「Continue」給 Bot，讓它重新開始。
- 用量檢查失敗時，「用量」會保留上次讀到的數字，並標示為「已過時」。

### en-US
- Set up a Codync Cloud computer step by step in one sheet: pick a provider, make and paste its key, then name it.
- Name your computers: in Manage computers, touch and hold a card, then Rename…
- When bots stop after 8 handoffs in a row, a Continue button in the chat gets them going again.
- When a usage check fails, Usage keeps the last numbers and marks them Out of date.

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
