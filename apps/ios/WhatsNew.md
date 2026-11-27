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

### en-US
- In a thread, what Claude Code runs in the background (a command, a workflow, a monitor) shows in a capsule above the message box: Background · what runs, such as the workflow's name and its current step, with the time. The task stays in progress until that work finishes; then Claude Code picks it up by itself, and its last reply is the task's report.
- Choosing a workspace folder looks and feels like iOS: the computer and your recent folders in a searchable list, Cancel and Use this folder at the top, and each folder opening as its own page when you browse.
