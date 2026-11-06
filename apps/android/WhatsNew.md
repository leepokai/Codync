<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.30.0

### zh-Hant
- 擴充功能現在有應用程式、代理與技能三個分頁。應用程式用瀏覽器登入或金鑰把服務連到你的電腦，也能加入任何 MCP 伺服器。
- 代理分頁有現成的代理，各有指示、技能與例行工作；點進去看內容，再按「加入代理」建立 Bot。
- 技能分頁依類別整理，包含完整的 OpenBot 技能；安裝後每個 Bot 都能使用。
- 現成的 Bot 與團隊移到代理分頁。

### en-US
- Extensions now has Apps, Agents and Skills tabs. Apps connect a service to your computer with a browser sign-in or a key, and custom connectors add any MCP server.
- The Agents tab has ready-made agents, each with its own instructions, skills and routines. Open one to see its work, then Add agent to create the bot.
- The Skills tab groups skills by category, including the whole OpenBot set. Every bot can use an installed skill.
- Ready-made bots and teams moved under Agents.

### zh-Hant
- 介面新增巴西葡萄牙文：可在設定選擇「Português (Brasil)」，或跟隨系統的葡萄牙文語言。
- 新增俄文介面：在設定選擇「Русский」，或跟隨系統的俄文語言。
- 用 QR Code 配對時，如果電腦當下沒能加入你的帳號（例如手機連不上 Codync 雲端，或電腦的雲端關著），下次連上時會自動補上。

### en-US
- Choose Português (Brasil) in Settings, or follow your Portuguese system language.
- Choose Русский in Settings, or follow your Russian system language.
- A computer you pair with a QR code that couldn't join your account at the time (the phone couldn't reach the Codync cloud, or the computer's cloud was off, say) joins it the next time it connects.
