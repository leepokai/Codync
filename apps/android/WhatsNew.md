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
- 在 thread 裡，Claude Code 在背景執行的指令、workflow 和監看會顯示在輸入框上方的膠囊：「背景 ·」加上正在跑的項目（例如 workflow 的名稱和目前步驟）與時間。背景工作跑完前，這個工作會一直保持進行中，跑完後 Claude Code 會自己接著處理，最後的回覆就是工作的報告。

### en-US
- In a thread, what Claude Code runs in the background (a command, a workflow, a monitor) shows in a capsule above the message box: Background · what runs, such as the workflow's name and its current step, with the time. The task stays in progress until that work finishes; then Claude Code picks it up by itself, and its last reply is the task's report.
