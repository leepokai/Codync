<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
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
- Bot 設定的「工作區」新增「專案」：Bot 一律在自己的資料夾（「Bot 的資料夾」，可以複製路徑），聊天、例行任務和檔案都在那裡；你的資料夾改列為專案，可以新增專案、設為預設或移除專案，第一個就是預設，thread 從預設專案開始。專案是 git 儲存庫時可以選「worktree 模式」：關、並行時或一律（每個 thread 都有自己的 worktree，你的 checkout 不會被動到）。原本在專案裡工作的 Bot 更新後會搬回自己的資料夾，那個專案成為它的第一個專案，專案裡的東西不會改動。
- Bot 設定裡 git 專案的那一列多了「共用資料夾」：列出的資料夾（例如 node_modules）會連結到每個新的 worktree，thread 不必重新安裝套件。專案最上層的 .worktreeinclude 列出的被 git 忽略的檔案（例如 .env）也會複製進新的 worktree。Bot 交派工作時可以替每個工作選擇在專案本身或自己的 worktree 進行，也能在工作進行中直接問 thread 問題、拿到回答。
- 新增 Threads 分頁：列出 Bot 所有的 thread，需要你處理的排最上面，接著是分組和最近的 thread。右上角的篩選按鈕可以「依資料夾分組」、「依 Bot 分組」或「依電腦分組」，也能「依狀態排序」或「依最近活動排序」；旁邊的搜尋可以用標題或 Bot 名稱找 thread，+ 會像電腦版一樣打開全螢幕的新 thread 頁面：預設不使用 Bot（在輸入框那一排選 agent 和模型），也可以改選一個 Bot，送出第一則訊息後頁面就變成這個 thread 的聊天畫面；長按 thread 可以停止、插隊、重試、複製標題或移除。分頁上的數字是正在處理的數量；原本 Bot 列表上方的「工作」按鈕已移除。
- Bot 分頁的篩選選單新增「依電腦分組」和「依狀態排序」；「將有新動態的機器人移到最前面」改成這裡的「依最近活動排序」，不再放在管理電腦裡。
- Bot 分頁的圖示改成機器人，和電腦版一致；右上角改成篩選、搜尋和新增三個按鈕，搜尋可以用名稱找 Bot，連線狀態移到標題下方，Threads 分頁的標題列也一樣，左上角是帳號按鈕；列表捲動時頂端改成柔和的模糊。
- 換 Bot 的 agent 或登入設定檔後，新的 agent 不再從零開始：下一則訊息會帶上目前對話的摘要。送出前聊天裡會出現「脈絡交接待進行」的分隔線，標出從哪個 agent 換到哪個，送出後可以點開看帶了什麼。
- 新增 Workflows 分頁（圖示和電腦版相同）：查看這台電腦的 workflow、它的步驟、觸發條件和執行紀錄，並直接核准或拒絕等待你核准的步驟。在 Bot 列表的電腦標題上點 workflow 圖示也能打開。右上角可以用標題搜尋 workflow，有多台電腦時可以切換電腦。等待核准的通知會直接打開那個步驟，在電腦版單獨執行的步驟也一樣。Workflow 在電腦版建立和編輯；長按 workflow 可以釘選到 Bot 列表上方。
- 用量畫面和小工具把 Claude 的用量標示為「Claude Code」，與 Bot 使用的 agent 名稱一致。
- 選 agent 時一起選登入設定檔：Bot 設定和 fallback 的 agent 清單裡，每個 agent 依登入設定檔各列一次，清單最後可以新增登入設定檔；原本獨立的「登入設定檔」列已移除。
- 聊天的輸入框改成上下兩層：上面打字，下面一排是加檔案的 +、Bot 目前的 agent 和模型、麥克風和送出鍵。點模型會打開模型面板，不必進 Bot 設定：上方選 agent，模型列滿整個畫面並附上說明（較舊的收在「更多模型」），下方一張卡片放推理強度、核准方式和快速模式。模型清單一打開就在，不用再等載入；往下拉可以重新讀取。
- 在中國大陸也能收到推播通知了。
- 點開 thread 會像電腦版一樣以聊天畫面全螢幕顯示：你的訊息和回覆是對話泡泡，需要你核准或選擇的卡片、檔案和 artifact 直接在 thread 裡處理，進行中的步驟顯示在工作中的那一行；右上角「詳情」可打開 Bot 的詳細資訊。返回和詳情是浮在頂端的玻璃按鈕，內容捲動時在頂端柔和地模糊。
- 電腦的「網路」頁會列出這台電腦上加了 Codync 的 agent（Claude Code、Codex 等）：標題、位址、是否正在執行，以及它們怎麼收信；較早的會話收在最後。
- Threads 分頁的每個 thread 開頭會顯示它正在使用的 coding agent 圖示；切換到備援 agent 時，顯示最後換到的那個。

### en-US
- Threads are no longer deleted a week after they end: scroll to the end of the Threads tab to load older ones, which open and take messages as before. To clean up on its own, pick 7, 30 or 90 days under Delete old threads for that computer in Computers; the default keeps them forever.
- A routine can name a folder: pick one under Folder in the routine editor and each run works there in a thread of its own; without one it runs in the bot's folder.
- Bot settings' Workspace now has Projects. A bot always lives in its own folder (Bot's folder, with a button to copy its path), where its chat, routines and files are; the folders it works on are its projects: add one, make another the default or remove one. Threads start in the default project. With a project in a git repository, Worktree mode picks Off, Side by side or Always (every thread gets its own worktree, so your checkout is never touched). A bot that worked in a project moves to its own folder, with that project as its first; nothing in the project changes.
- A git project's row in bot settings adds Shared folders: the folders listed (node_modules, say) are linked into each new worktree, so a thread doesn't install packages again. Ignored files listed in a .worktreeinclude file at the top of the project (such as .env) are copied in too. A bot can choose, for each task it hands out, the project itself or a worktree of its own, and ask a thread a question while it works and get its answer.
- A new Threads tab lists every thread of your bots: what needs you on top, then groups, then recent ones. Its filter button groups them by folder, bot or computer and sorts them by status or last active; Search beside it finds threads by title or bot, and + opens a full-screen page for a new thread, like the desktop's: without a bot by default (pick its agent and model in the message box's row), or with a bot you choose; send its first message and the page becomes the thread's chat; touch and hold a thread to stop it, let it cut in, try it again, copy its title or drop it. Its badge counts what is being worked on. The Work button at the top of the bot list is gone.
- The Bots tab's filter menu adds Group by computer and Sort by status. Move active bots to the top is now Sort by last active there, no longer in Manage computers.
- The Bots tab has the robot icon the desktop app uses, and its top right holds Filter, Search and New: search finds bots by name, the connection status sits under the title (on the Threads tab too, beside the account button), and the list now fades softly under the top bar as it scrolls.
- Changing a bot's agent or login profile no longer starts it from nothing: your next message carries a summary of the conversation. Until then a divider in the chat reads Context handoff pending with both agents; afterwards tap it to see what was carried.
- A Workflows tab, with the desktop's icon: see a computer's workflows, their steps, triggers and run history, and approve or reject a step that waits for you. The workflow icon on a computer's heading in the bot list opens them too. Search at the top right finds a workflow by title, and with more computers than one the filter button switches computer. An approval's notification opens that step, a step run on its own in the desktop app too. Workflows are built and edited in the desktop app. Long-press a workflow to pin it above the bots.
- Usage and widgets name Claude's usage "Claude Code", the same name bots use for that agent.
- Pick an agent and its login profile in one step: a bot's and each fallback's agent list shows every agent once per login profile and ends with Add login profile. The separate Login profile row is gone.
- The message box now has two layers: you type on top, and under it are the + for files, the bot's agent and model, the mic and send. Tap the model for the model panel, without opening the bot's settings: the agent at the top, its models full width with what each is for (older ones under More models), and one card with effort, Approvals and Fast mode. The list is there the moment it opens; pull down to read it again.
- Push notifications now reach phones and computers in mainland China.
- A thread now opens full screen as a chat, like on the desktop: your messages and its replies as bubbles, its approval and choice cards, files and artifacts right there to answer and open, and its steps in the working line while it works; Details at the top right opens the bot's details. Back and Details float at the top as glass buttons, and the thread fades softly under them as it scrolls.
- The computer's Network page lists the agents (Claude Code, Codex and others) that use Codync there: each session's title, address, whether it's running and how it gets its mail, with older sessions folded at the end.
- Each thread on the Threads tab starts with the icon of the coding agent it runs on; after it falls back, the one it switched to.

## 2.53.0
### zh-Hant
- Claude Code 的 Bot 使用 subagent 時，展開的工作步驟、「完整對話」和 task 的 thread 裡，subagent 做的步驟會縮排在啟動它的那一步底下；那一步也會顯示 subagent 的類型、用了多少 token 和花了多久，在背景執行的則顯示「在背景執行」。

### en-US
- When a Claude Code bot uses subagents, each subagent's steps sit indented under the step that started it, in the unfolded working steps, Full conversation and a task's thread. That step also shows the subagent's type, the tokens it used and how long it took, or "in background" for one that runs in the background.

## 2.52.0
### zh-Hant
- 有些 agent（例如 Hermes）會在回覆中途問你問題，現在這些問題會顯示在聊天的選擇卡片上，等你回答，不會再被「自動核准」替你選第一個選項。有選項的問題只能選其中一個，卡片上不再顯示「其他答案」那一列。
- Bot 設定的 Agent 卡片新增「顧問模式」：聊天負責規劃與分派工作，thread 可以改用別的 agent、模型或登入設定檔，也有自己的備用列表。
- 等你核准或選擇的 thread 不再佔名額，下一件 task 可以先開始；你回覆後它會接著做。
- 你在聊天裡補充正在進行的 task，Bot 會把話轉給那個 thread，task 做完仍只回報一次。
- 寫給 thread 的訊息也可以附上照片和檔案，和聊天一樣。

### en-US
- Some agents, such as Hermes, ask you questions mid-reply. These now wait for you on the chat's choice card, and Approve automatically no longer picks their first option for you. A question with options takes one of them, so its card has no Something else row.
- Advisor mode in a bot's Agent settings: the chat plans and hands work on, and its threads can run on another agent, model or login profile, with fallbacks of their own.
- A thread waiting for your approval or choice no longer holds a place, so the next task can start; it carries on once you answer.
- What you add in the chat about a task under way, the bot passes on to its thread, and the task still reports once.
- Messages to a thread can carry photos and files too, as in the chat.

## 2.51.0
### zh-Hant
- Bot 離開聊天做的事現在有兩個清楚的名稱：一件工作叫 task，Bot 做 task 的地方叫 thread，有自己的工作階段。一個 thread 一次做一件 task，做完可以接下一件。
- 聊天上方的「背景」那一行改名為「工作」，和「工作」頁同名。
- thread 都在忙、要選一件讓位時，提示改成「所有 thread 都在忙」；停止一件工作時問的是「停止這個 task？」。
- 配對說明改成「配對手機」，和電腦選單上的名稱一致：同一個碼 iPhone 和 Android 手機都能配。
- 第一次設定手機時，可以直接建立一臺 Codync Cloud 雲端電腦，不用先在自己的電腦上安裝 Codync；設定會等它準備好，再進到你的 Bot。

### en-US
- What a bot does away from the chat now has two clear names: a task is one piece of work, and a thread is where the bot does tasks, with a session of its own. A thread does one task at a time and may go on to the next.
- The "In the background" line above the chat is now "Work", like the Work page.
- When every thread is busy and one task has to give way, the prompt says so; stopping one asks "Stop this task?".
- The pairing instructions now say Pair phone, the name the computer's menu uses, since the same code pairs iPhone and Android phones.
- First-time setup can start a Codync Cloud computer instead of installing Codync on one of yours, then waits until it's ready and opens your bots.

## 2.50.0
### zh-Hant
- 建立 Codync Cloud 雲端電腦改成在同一個畫面一步一步來：先選供應商（看得到在哪裡執行、要多少錢），再照著步驟到供應商網站建立金鑰並貼上，最後取名、建立。
- 可以替自己的電腦取名：在「管理電腦」長按電腦的卡片，選「重新命名…」。所有 App 都會顯示這個名稱，同一台機器上的兩個帳號也分得出來；留空就改回主機名稱。
- Bot 之間互相交接到上限（連續 8 次）而停下時，聊天裡會出現一則附「繼續」按鈕的提示：點一下就會替你傳「Continue」給 Bot，讓它重新開始。
- 用量檢查失敗時，「用量」會保留上次讀到的數字，並標示為「已過時」。

### en-US
- Setting up a Codync Cloud computer now goes step by step in one sheet: pick a provider (where it runs, what it costs), follow the steps to make its key and paste it, then name it and create it.
- Name your own computers: in Manage computers, touch and hold a computer's card, then Rename…. Every app shows that name, so two accounts on one machine are easy to tell apart; leave it empty to go back to the hostname.
- When bots hand work to each other 8 times in a row and stop, the chat shows a note with a Continue button: one tap sends the bot "Continue" from you so it can go on.
- When a usage check fails, Usage keeps the last numbers and marks them Out of date.

## 2.49.3
### zh-Hant
- Bot 預設的工作區改名為「專屬資料夾」。
- 電腦有安裝 Git 時，Codync 會在每一輪對話前後替 Bot 的專屬資料夾留一個版本：可以請 Bot 說明某一輪改了什麼，或把檔案還原成之前的樣子。「設定」的「工作區」下方會顯示 Git repository。
- 在專屬資料夾裡，Bot 的背景工作都在同一個資料夾進行，所以那裡不再顯示「每個 thread 一個 worktree」開關。
- Bot 在背景做的每件工作現在一律叫 thread：「設定」裡的「同時幾個 agent」改名為「同時幾個 thread」，「每個 agent 一個 worktree」改名為「每個 thread 一個 worktree」。
- 主畫面的 Codync 圖示可以顯示未讀數量，和 App 內各聊天的未讀數相加一致，讀完、或打開 App 時就會更新。預設關閉，可在「設定」→「通知」開啟「在 App 圖示上顯示未讀數」。
- Bot 在做事時旁邊的小圖示換成跟頭像同一種點陣：工作中、搜尋、等你回覆、連線和編輯檔案各有自己的動作，小尺寸也看得清楚。
- 動態島上工作中的 Bot 臉和旁邊的小圖示，點陣現在會對齊，不再錯位。
- 需要更新時，更新卡片上的「立即更新」「更新 Codync」和「更新」按鈕會跟著 App 語言顯示。
- 從睡眠中醒來的 Codync Cloud 雲端電腦會顯示「正在喚醒電腦…」，不再顯示離線或連不上；這段時間傳的文字訊息會等它醒來再送出。

### en-US
- A bot's default workspace is now called Own folder.
- With Git installed, Codync keeps a version of a bot's own folder before and after each turn: ask the bot what a turn changed, or to bring a file back as it was. Settings show Git repository under Workspace.
- In its own folder, a bot's threads all work in that one folder, so Settings no longer show Worktree per thread there.
- Each piece of work a bot does in the background is now called a thread everywhere: in Settings, Agents at once is now Threads at once, and Worktree per agent is now Worktree per thread.
- The Codync icon on the Home Screen can show your unread count, the same number your chats' badges add up to; it updates as you read them and whenever you open the app. It's off by default: turn on Unread count on app icon in Settings → Notifications.
- The small icon beside a busy bot now uses the same dot grid as the avatars: working, searching, waiting for you, connecting and editing files each move their own way, and stay clear at small sizes.
- In the Dynamic Island, a working bot's face and the icon beside it now draw their dots lined up instead of shifted.
- The buttons on the update cards (Update now, Update Codync, Update) now show in the app's language.
- A Codync Cloud computer waking from sleep now shows Waking the computer… instead of offline or can't reach; a text message you send meanwhile goes out once it's up.

## 2.49.1
### zh-Hant
- Bot 會把比較久的工作交給背景進行，而且可以同時做好幾件：聊天不會被占住，你可以繼續傳訊息，它也會照常回答。
- 輸入框上方會有一個「背景」膠囊，顯示幾件進行中、幾件等待中；只有一件在做時會顯示它的進度與時間，也可以直接停止（已經改的內容會留著）。點一下會打開 Bot 的「詳情」。
- 「詳情」現在列出 Bot 在做的事：「Threads」是正在進行的工作和還空著的名額，點一列會打開那個 thread 的頁面，看它做了什麼，也可以像聊天一樣直接傳訊息給它：它還在做時訊息會先排隊，可以「現在送」或「移除」；已經結束的 thread 收到訊息會再繼續。每個 thread 會顯示它工作的資料夾或分支；「Event Box」是依序等待的工作，可以「先做」、名額滿時「插隊」（選一件讓位，它之後會重新開始），或長按「丟掉」。
- 「最近」列出一天內結束的工作：完成的可以點開看過程，停止或失敗的會說明原因，並且可以「再試一次」。
- 「同時幾個 agent」在 Bot 的「設定」裡，可以設 1 到 6 個。
- 「設定」的「工作區」會說明資料夾是不是 Git repository，旁邊的資訊按鈕會打開說明文件。
- 「指令」和「智慧體」（模型、快速模式、備援等）從「詳情」移到 Bot 的「設定」最上方。
- 背景工作需要你核准時，卡片會出現在聊天裡並標示「背景」，照平常的方式回答就好。
- 背景工作完成後，Bot 會用一則訊息告訴你結果；沒有完成也會說明原因。
- 例行任務和其他 Bot 的請求現在也在背景進行，不再占住聊天：例行任務執行時，你可以繼續和 Bot 對話。
- Bot 列表上方新增「工作」按鈕（有背景工作時才會出現）：一次看到每個 Bot 正在進行和等待中的背景工作，操作和「詳情」裡一樣。
- 背景工作可以在聊天裡問你問題（選擇卡片）、傳檔案或語音給你，並標示「背景」和那件工作的名稱。
- 「每個 agent 一個 worktree」在 Bot 的「設定」裡（資料夾是 Git repository 時才會出現）：同時在同一個 repository 工作的 agent 各有自己的分支，不會互相改到檔案。Codync 不會合併或刪除有內容的分支。
- 背景工作還在進行時，「實時活動」會一直留著，需要你時也會顯示。
- 從任何 App 的分享選單選「Codync」，就能把文字、連結、照片或檔案傳給 Bot，還能附上一段訊息。上次分享的對象會排在「最近」並預先選好。
- 更新提示會清楚指出需要更新這個 App，還是電腦上的 Codync；雲端電腦更新時也會顯示進度說明。
- 智慧體可以有多個「登入設定」：在智慧體設定頁的「登入設定」新增賬戶（Claude Code、Codex、OpenCode、Pi、Google Antigravity 等可在同一台電腦登入多個賬戶），或為任何智慧體新增 API 金鑰（自行輸入環境變數，或從電腦的登入 shell 匯入；值只留在電腦上）。每個登入設定（包含預設的）都能自己取名，再到 bot 設定的「智慧體」選擇它要用的登入設定，兩個 bot 就能同時用不同的登入工作。
- 備援可以選同一個智慧體的另一個登入設定：一個的額度用完，bot 會換到下一個繼續。
- 「用量上限」會在智慧體底下分別列出每個賬戶的額度（Claude Code、Codex），小工具顯示最接近上限的那一個。
- 電腦重新啟動或 App、Codync 更新後，iPhone 只接收新的訊息，不再重新載入每個聊天。
- 選擇卡片改用編號選項；最後一列「其他答案」可以直接打字回答，bot 會帶著你的話繼續。
- Bot 說「晚點回來看」時會真的設定跟進：聊天裡會出現「20 分鐘後跟進」的提示，時間到了它會自己回到聊天告訴你結果，還沒有進展就不打擾你。等待中的跟進會列在例行任務上方，可以直接取消。
- 例行任務會記住上次做到哪裡，分批進行的工作下次會接著做。
- 語音通話設定的 OpenAI 區塊會顯示電腦目前使用的「伺服器」：OpenAI，或你在電腦上設定的自有語音伺服器。使用自有伺服器時，模式固定為語音識別。
- 可以只回覆訊息的其中一段：長按訊息選「選取文字」，選好文字後在選單點「回覆」，Bot 會知道你問的是哪一段。
- Bot 也可以引用訊息：晚一點才回的答案，上方會顯示它回答的是哪一則。點一下引用就會捲到那則訊息。
- Bot 設定的「技能」只列出名稱，點旁邊的資訊按鈕可以看完整說明。
- 裝置上聽寫與語音通話不會再弄丟停頓前說的話。
- Bot 改用備援時，「智慧體」卡片會說明原本的智慧體為什麼停下（已登出、被限速等），只有用量上限才顯示重設時間；這一行和備援底下的休息時間都會跟著 App 的語言。
- 輸入 `/` 時，清單最後會列出 Bot 所用智慧體自己的指令（Claude Code、Codex、OpenCode 等），和 Codync 自己的指令、技能分開，標上智慧體的名稱、圖示和說明；選一個就會填入那個指令，送出後由智慧體直接執行。
- 電腦的「網路」頁會列出新的規則等級：「自動」只讓其他電腦的 Bot 做 agent 判斷為安全的事，其餘先問你；「詢問」讓每個變更和指令都等你批准。沒有符合任何規則的電腦預設為「自動」，原本就在網路裡的電腦維持「允許」。
- 備援直接在「智慧體」卡片裡設定，和 Bot 自己的智慧體一樣：點一列就能換它的智慧體、模型或登入設定，順序不變；按「+」新增的備援會直接展開。
- 聽寫中直接按「傳送」，會等說的話轉成文字，再和打好的字一起送出，不會再丟掉。
- 等待核准的卡片會一直算在 Bot 的未讀數字裡（顯示為琥珀色），打開聊天或「標示為已讀」都不會讓它消失，直到你回覆為止；收起的分組裡有 Bot 在等你時，分組的數字也會變成琥珀色。
- 長按 Bot 選「新會話」，就能讓智慧體從新的上下文開始，聊天記錄會保留。
- 背景工作也會用備援；需要登入時會在聊天裡請你登入。
- 轉寫中、資料夾、雲端電腦更新等幾段介面文字，現在會以你選的語言顯示。

### en-US
- Bots do long work in the background, several things at once, and keep answering in the chat.
- A capsule above the message box says how many are working and waiting (one alone: what it's doing, with Stop). Tap it for the bot's Details.
- Details lists the bot's work. Threads: what runs now; tap one to write to it like a chat, even once it's done. Each shows its folder or branch. Event box: what waits, with First, Cut in and Drop. Recent: what ended, with Try again.
- Agents at once, in the bot's Settings, sets how many work at a time (1 to 6). Instructions and Agent (model, fast mode, fallbacks) moved there from Details.
- Background work can ask for approval or a choice and send files or voice notes in the chat, marked "In the background".
- Routines and other bots' requests run in the background too.
- Settings > Workspace says if the folder is a git repository.
- A Work button at the top of the bot list shows every bot's running or waiting work.
- Worktree per agent, in the bot's Settings (for a Git repository): agents working at once each get their own branch.
- The Live Activity stays while background work runs, and shows when it needs you.
- Share text, links, photos or files to a bot from any app: choose Codync in the share sheet, with a message if you like. Bots you shared to last are under Recent.
- Update notices say whether this app or Codync on your computer needs updating, and when a cloud computer is updating itself.
- Agents can have several login profiles: add an account or an API key in an agent's setup (keys stay on the computer) and pick a bot's under Agent.
- A fallback can be the same agent on another login profile: when one runs out of usage, the bot carries on with the next.
- Usage limits lists each account of an agent with its own limits (Claude Code, Codex), and widgets show the one nearest its limit.
- After a restart or an update, the iPhone loads only what's new, not every chat again.
- Choice cards show numbered options; the last row, Something else, takes your own answer.
- A bot that says it will check back later sets a follow-up ("Follow-up in 20 minutes") and tells you what it found when it's due. Waiting ones are above its routines.
- A routine remembers where its last run stopped, so work done in parts carries on from there.
- Voice chat settings show which Server the computer's OpenAI engine uses: OpenAI, or your own speech server (Mode then stays on Speech).
- Reply to one part of a message: touch and hold it, choose Select Text, select the words, then Reply.
- Bots can quote a message too: tap the quote above an answer to see it.
- A bot's Skills settings list only each skill's name; the info button beside one shows its description.
- On-device dictation and calls keep the words you said before a pause.
- On a fallback, the Agent card says why the bot's agent stopped (signed out, rate limited, etc.).
- Typing `/` also lists the agent's own commands (Claude Code, Codex, OpenCode and more) under its name; pick one and the agent runs it when you send.
- Network rules gain two levels: Auto (another computer's bots get only what the agent judges safe; the rest asks you) and Ask (everything waits for you). A computer no rule matches gets Auto.
- Fallbacks are set right in the Agent card, like the bot's own agent: tap one to change its agent, model or login profile in place; + adds one already open.
- Send while dictating waits for what you said and sends it with what you typed.
- A waiting approval card stays in the bot's badge, in amber, until you answer it.
- Touch and hold a bot for New session: a fresh context for its agent; the chat stays.
- Background work uses fallbacks too; sign-ins it needs show in the chat.
- A few more interface texts (transcribing, folders, cloud computer updates) now show in your language.
