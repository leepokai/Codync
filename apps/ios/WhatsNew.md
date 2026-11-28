<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.55.0
### zh-Hant
- 在 thread 裡，Claude Code 在背景執行的指令、workflow 和監看會顯示在輸入框上方的膠囊：「背景 ·」加上正在跑的項目（例如 workflow 的名稱和目前步驟）與時間。背景工作跑完前，這個工作會一直保持進行中，跑完後 Claude Code 會自己接著處理，最後的回覆就是工作的報告。
- 選擇工作資料夾的畫面改成 iOS 原生樣式：電腦和最近使用的資料夾在可搜尋的清單裡，「取消」和「使用此資料夾」在上方，瀏覽時每個資料夾各自開成一頁。
- 沒有 bot 的 thread 若使用 Codync 為它建立的資料夾，現在會顯示為「專屬資料夾」而不是一串 ID；在 Threads 依資料夾分組時，這些 thread 都會集中在同一個「專屬資料夾」群組。
- 由其他 App 透過 MCP 開始的 thread（例如電腦上的 coding agent 或雲端助理），會在標題下方標示是哪個 App，例如「由 Cursor 啟動（MCP）」。這些 App 在電腦版的「電腦」裡的「MCP 存取」設定，核准一律留給你在 Codync 裡處理。
- 手機也能看到哪些 App 可以透過 MCP 使用你的電腦：在「管理電腦」裡該電腦的「MCP 存取」可以看到每個 App 的等級和到期日，並直接撤銷，它的權杖會立刻失效。新增 App 和調整權限仍在電腦上進行。

### en-US
- In a thread, what Claude Code runs in the background (a command, a workflow, a monitor) shows in a capsule above the message box: Background · what runs, such as the workflow's name and its current step, with the time. The task stays in progress until that work finishes; then Claude Code picks it up by itself, and its last reply is the task's report.
- Choosing a workspace folder looks and feels like iOS: the computer and your recent folders in a searchable list, Cancel and Use this folder at the top, and each folder opening as its own page when you browse.
- A thread without a bot that works in the folder Codync made for it now shows Own folder instead of an ID, and grouped by folder, all such threads sit together in one Own folder group.
- A thread another app started over MCP (a coding agent on your computer, or a cloud assistant) says which app under its title, for example "Started by Cursor (MCP)". You set what these apps may do under MCP access on the desktop's Computers page; approvals always stay with you in Codync.
- See which apps may use your computer over MCP from your phone: MCP access under the computer in Manage computers lists each app with its level and expiry, and revokes one right away. Adding apps and changing what they may do stays on the computer.
