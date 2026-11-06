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
- Agent、模型、推理強度與 Fast mode 整合成可收合的精簡設定；每個 Bot 可獨立保存推理強度，Default 也會顯示預設值。
- 「設定」現在是「狀態」右邊的獨立分頁，只放這支手機自己的設定。電腦的擴充套件、語音、螢幕、推送通知與連線，都移到「管理電腦」裡那台電腦的卡片上；推送通知可以逐台電腦開關。
- 電腦卡片最下方的「Codync」會顯示那台電腦的版本；有新版時可以直接點「更新」。
- 電腦卡片多了網路按鈕（Beta）：一台電腦可加入多個網路，列出各網路的電腦、是否在線、提供的 Bot、誰能傳訊息給這裡的 Bot，以及雲端助理是否開啟。Bot 設定多了「提供給網路」與「雲端助理可聯繫」開關。

### en-US
- Agent, model, effort and Fast mode share compact settings; each bot keeps its own effort.
- Settings is its own tab; each computer's Extensions, Voice chat, Screen, Push (now per computer) and Connection moved to its card in Manage computers.
- The card shows your computer's Codync version, with Update when a newer one is out.
- Network (beta) on the card lists your networks' computers and bots; bot settings add Offered to the network and Reachable by cloud assistants.
