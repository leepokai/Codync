<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.39.0

### zh-Hant
- 擴充功能現在是市集：應用程式、代理程式與技能三個分頁，附精選項目與分類。
- 代理程式會以自己的指令、外觀、技能與例行工作建立 Bot；先讀它的頁面再新增。
- 技能隨 App 內建、可離線安裝；目錄中的應用程式一步就能加入對應的連接器。

### en-US
- Extensions is now a marketplace with Apps, Agents and Skills, featured picks and categories.
- Adding an agent creates a bot with its instructions, look, skills and routines; read its page first.
- Bundled skills install offline, and catalogued apps add their connector in one step.

## 2.32.0

### zh-Hant
- Bot 清單可以分成自己的分組，例如「工作」和「個人」：長按 Bot 選「移到分組」，點分組標題即可收合，收合時會顯示裡面的未讀數。
- 遠端畫面連線更快，網路短暫中斷時也不會斷掉。

### en-US
- Sort the bot list into your own sections, like Work and Personal: touch and hold a bot and choose Move to section, then tap a section's heading to fold it, with its unread count showing while it's folded.
- The remote screen connects faster and stays up through a brief connection drop.
