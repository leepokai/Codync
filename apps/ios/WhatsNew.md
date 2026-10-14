<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice. tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.16.1

### zh-Hant

- 電腦上沒有 Node.js 時，Extensions 會標出需要它的 agent，按一下就能讓 Codync 幫那台電腦加上。
- Windows 電腦上的 Claude Code、Codex 等 agent 現在可以正常啟動了。
- Cursor bot 可以替模型選擇推理強度、快速模式等設定，不再只能用每個模型的預設組合。
- Extensions 的已安裝數量明確標示只計入連接器與技能，並改善小螢幕上的標題排列。
- App 當機時會隨使用情況分析送出當機報告，幫我們更快修好問題；報告不含任何對話或檔案內容。可隨時在「設定」關閉使用情況分析。

### en-US

- When your computer doesn't have Node.js, Extensions shows which agents need it, and one tap has Codync add it.
- Claude Code, Codex and other agents now start on Windows computers.
- Cursor bots can now choose a model's settings, such as reasoning effort and fast mode, instead of only its default variant.
- Extensions now labels its installed count as connectors and skills, with a clearer header on smaller screens.
- Crash reports are now sent with usage analytics so we can fix crashes faster; they contain none of your conversations or files. Turn usage analytics off anytime in Settings.
