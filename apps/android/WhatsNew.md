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
- 新的 Workflow Builder 會把觸發條件、步驟與結果建成流程，步驟會依次出現在畫布上。
- 每個 Bot 可以在設定中命名瀏覽器設定檔，讓登入與 Cookie 保留在該設定檔裡。
- Slack、Telegram 與 Discord 的提及可以啟動流程；GitHub 的 issue、PR 與失敗檢查也一樣。
- 新增 TickTick 等應用程式，以及每個代理程式的建立者、審核狀態、分類與更新日期。

### en-US
- Extensions is now a marketplace with Apps, Agents and Skills, featured picks and categories.
- Adding an agent creates a bot with its instructions, look, skills and routines; read its page first.
- Bundled skills install offline, and catalogued apps add their connector in one step.
- The new Workflow Builder turns a trigger, steps and a result into a workflow that draws itself on the canvas.
- Each bot can name a browser profile, keeping sign-ins and cookies inside that profile.
- Slack, Telegram and Discord mentions can start a workflow, as can GitHub issues, pull requests and failed checks.
- New apps such as TickTick, and each agent page now names its creator, review, category and update date.

## 2.32.0

### zh-Hant
- Bot 清單可以分成自己的分組，例如「工作」和「個人」：長按 Bot 選「移到分組」，點分組標題即可收合，收合時會顯示裡面的未讀數。
- 遠端畫面連線更快，網路短暫中斷時也不會斷掉。

### en-US
- Sort the bot list into your own sections, like Work and Personal: touch and hold a bot and choose Move to section, then tap a section's heading to fold it, with its unread count showing while it's folded.
- The remote screen connects faster and stays up through a brief connection drop.
