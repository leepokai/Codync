<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.55.0
### zh-Hant
- 由其他 App 透過 MCP 開始的 thread（例如電腦上的 coding agent 或雲端助理），會在標題下方標示是哪個 App，例如「由 Cursor 啟動（MCP）」。這些 App 在電腦版的「電腦」裡的「MCP 存取」設定，核准一律留給你在 Codync 裡處理。
- 手機也能看到哪些 App 可以透過 MCP 使用你的電腦：在「管理電腦」裡該電腦的「MCP 存取」可以看到每個 App 的等級和到期日，並直接撤銷，它的權杖會立刻失效。新增 App 和調整權限仍在電腦上進行。

### en-US
- A thread another app started over MCP (a coding agent on your computer, or a cloud assistant) says which app under its title, for example "Started by Cursor (MCP)". You set what these apps may do under MCP access on the desktop's Computers page; approvals always stay with you in Codync.
- See which apps may use your computer over MCP from your phone: MCP access under the computer in Manage computers lists each app with its level and expiry, and revokes one right away. Adding apps and changing what they may do stays on the computer.

## 2.54.0
### zh-Hant
- Thread 不再於結束一週後被刪除：在 Threads 分頁捲到最底就會載入更早的 thread，可以照常打開和繼續傳訊息。想自動清理的話，在「電腦」裡該電腦的「刪除舊的 thread」選 7、30 或 90 天後，預設是永久保留。
- 例行工作可以指定資料夾：在例行工作編輯器的「資料夾」選一個資料夾，每次執行都會在那個資料夾裡用自己的 thread 進行；不選就在 Bot 的資料夾。
- Bot 設定的「工作區」新增「專案」：Bot 一律在自己的資料夾，thread 在預設專案工作；專案是 git 儲存庫時可選「worktree 模式」。原本在專案裡的 Bot 會搬回自己的資料夾，專案保留為第一個。
- git 專案新增「共用資料夾」：列出的資料夾（例如 node_modules）會連結到每個新的 worktree，不必重新安裝套件。
- 新增 Threads 分頁：列出 Bot 所有的 thread，需要你處理的排最上面，接著是分組和最近的 thread。右上角的篩選按鈕可以「依資料夾分組」、「依 Bot 分組」或「依電腦分組」，也能「依狀態排序」或「依最近活動排序」；旁邊的搜尋可以用標題或 Bot 名稱找 thread，+ 會像電腦版一樣打開全螢幕的新 thread 頁面：預設不使用 Bot（在輸入框那一排選 agent 和模型），也可以改選一個 Bot，送出第一則訊息後頁面就變成這個 thread 的聊天畫面；長按 thread 可以停止、插隊、重試、複製標題或移除。分頁上的數字是正在處理的數量；原本 Bot 列表上方的「工作」按鈕已移除。
- Bot 分頁的篩選選單新增「依電腦分組」和「依狀態排序」；「將有新動態的機器人移到最前面」改成這裡的「依最近活動排序」，不再放在管理電腦裡。
- Bot 分頁的圖示改成機器人，和電腦版一致；右上角改成篩選、搜尋和新增三個按鈕，搜尋可以用名稱找 Bot，連線狀態移到標題下方；Threads 分頁也一樣。
- 換 Bot 的 agent 或登入設定檔後，新的 agent 不再從零開始：下一則訊息會帶上目前對話的摘要。送出前聊天裡會出現「脈絡交接待進行」的分隔線，標出從哪個 agent 換到哪個，送出後可以點開看帶了什麼。
- 新增 Workflows 分頁（圖示和電腦版相同）：查看這台電腦的 workflow、它的步驟、觸發條件和執行紀錄，並直接核准或拒絕等待你核准的步驟。右上角可以用標題搜尋 workflow，有多台電腦時可以切換電腦。等待核准的通知會直接打開那個步驟，在電腦版單獨執行的步驟也一樣。Workflow 在電腦版建立和編輯；長按 workflow 可以釘選到 Bot 列表上方。
- 用量畫面和小工具把 Claude 的用量標示為「Claude Code」，與 Bot 使用的 agent 名稱一致。
- 在 Bot 列表長按刪除 Bot 或群組聊天，確認後會真的刪掉。
- 選 agent 時一起選登入設定檔：Bot 設定和 fallback 的 agent 清單裡，每個 agent 依登入設定檔各列一次，清單最後可以新增登入設定檔；原本獨立的「登入設定檔」列已移除。
- 聊天的輸入框改成上下兩層：上面打字，下面一排是加檔案的 +、Bot 目前的 agent 和模型、麥克風和送出鍵。點模型會打開模型面板，不必進 Bot 設定：上方選 agent，模型列滿整個畫面並附上說明（較舊的收在「更多模型」），下方一張卡片放推理強度、核准方式和快速模式。模型清單一打開就在，不用再等載入；往下拉可以重新讀取。
- 在中國大陸也能收到推播通知了。
- 點開 thread 會像電腦版一樣以聊天畫面全螢幕顯示：訊息和回覆是對話泡泡，卡片、檔案和 artifact 直接在 thread 裡處理；右上角「詳情」可打開 Bot 的詳細資訊。返回和詳情是浮在頂端的玻璃按鈕，內容捲動時在頂端柔和地模糊。
- 電腦的「網路」頁會列出這台電腦上的 agent 會話（Claude Code、Codex 等）：標題、位址和是否正在執行。
- Threads 分頁的每個 thread 開頭會顯示它正在使用的 coding agent 圖示；切換到備援 agent 時，顯示最後換到的那個。

### en-US
- Threads are no longer deleted a week after they end: scroll to the end of the Threads tab to load older ones, which open and take messages as before. To clean up on its own, pick 7, 30 or 90 days under Delete old threads for that computer in Computers; the default keeps them forever.
- A routine can name a folder: pick one under Folder in the routine editor and each run works there in a thread of its own; without one it runs in the bot's folder.
- Bot settings add Projects: a bot always lives in its own folder, and threads work in its default project. Worktree mode (Off, Side by side, Always) shows for git projects. Bots that worked in a project move home, the project kept as their first.
- Git projects add Shared folders: folders like node_modules are linked into each new worktree, so packages aren't installed again.
- A new Threads tab lists every thread of your bots: what needs you on top, then groups, then recent ones. Its filter button groups them by folder, bot or computer and sorts them by status or last active; Search beside it finds threads by title or bot, and + opens a full-screen page for a new thread, like the desktop's: without a bot by default (pick its agent and model in the message box's row), or with a bot you choose; send its first message and the page becomes the thread's chat; touch and hold a thread to stop it, let it cut in, try it again, copy its title or drop it. Its badge counts what is being worked on. The Work button at the top of the bot list is gone.
- The Bots tab's filter menu adds Group by computer and Sort by status. Move active bots to the top is now Sort by last active there, no longer in Manage computers.
- The Bots tab has the robot icon the desktop app uses, and its top right holds Filter, Search and New: search finds bots by name, and the connection status sits under the title, on the Threads tab too.
- Changing a bot's agent or login profile no longer starts it from nothing: your next message carries a summary of the conversation. Until then a divider in the chat reads Context handoff pending with both agents; afterwards tap it to see what was carried.
- A Workflows tab, with the desktop's icon: see a computer's workflows, their steps, triggers and run history, and approve or reject a step that waits for you. Search at the top right finds a workflow by title, and with more computers than one the filter button switches computer. An approval's notification opens that step, a step run on its own in the desktop app too. Workflows are built and edited in the desktop app. Long-press a workflow to pin it above the bots.
- Usage and widgets name Claude's usage "Claude Code", the same name bots use for that agent.
- Deleting a bot or group chat from its long-press menu in the bot list works again.
- Pick an agent and its login profile in one step: a bot's and each fallback's agent list shows every agent once per login profile and ends with Add login profile. The separate Login profile row is gone.
- The message box now has two layers: you type on top, and under it are the + for files, the bot's agent and model, the mic and send. Tap the model for the model panel, without opening the bot's settings: the agent at the top, its models full width with what each is for (older ones under More models), and one card with effort, Approvals and Fast mode. The list is there the moment it opens; pull down to read it again.
- Push notifications now reach phones and computers in mainland China.
- A thread now opens full screen as a chat, like on the desktop: messages and replies as bubbles, its cards, files and artifacts right there; Details at the top right opens the bot's details. Back and Details float at the top as glass buttons, and the thread fades softly under them as it scrolls.
- A computer's Network page lists its agents' sessions (Claude Code, Codex…) and whether they run.
- Each thread on the Threads tab starts with the icon of the coding agent it runs on; after it falls back, the one it switched to.

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
