<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.29.0

### zh-Hant
- 對話輸入框右側新增上下文用量圓環；點一下可查看目前 Agent 工作階段已用與可用的 token 數。
- Agent、模型、推理強度與 Fast mode 整合成可收合的精簡設定；每個 Bot 可獨立保存推理強度，Default 也會顯示預設值。
- 「設定」現在是「狀態」右邊的獨立分頁，只放這支手機自己的設定。電腦的擴充套件、語音、螢幕、推送通知與連線，都移到「管理電腦」裡那台電腦的卡片上；推送通知可以逐台電腦開關。
- 電腦卡片最下方的「Codync」會顯示那台電腦的版本；有新版時可以直接點「更新」。
- 電腦卡片多了網路按鈕（Beta）：一台電腦可加入多個網路，列出各網路的電腦、是否在線、提供的 Bot、誰能傳訊息給這裡的 Bot，以及雲端助理是否開啟。Bot 設定多了「提供給網路」與「雲端助理可聯繫」開關。

### en-US
- A context ring beside the message box shows your agent session’s usage; tap for token counts.
- Agent, model, effort and Fast mode share compact settings; each bot keeps its own effort.
- Settings is its own tab; each computer's Extensions, Voice chat, Screen, Push (now per computer) and Connection moved to its card in Manage computers.
- The card shows your computer's Codync version, with Update when a newer one is out.
- Network (beta) on the card lists your networks' computers and bots; bot settings add Offered to the network and Reachable by cloud assistants.
