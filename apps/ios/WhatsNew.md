<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.29.0

### zh-Hant
- 對話輸入框右側新增上下文用量圓環；點一下可查看目前 Agent 工作階段已用與可用的 token 數。
- Agent、模型、推理強度與 Fast mode 整合成可收合的精簡設定；每個 Bot 可獨立保存推理強度，Default 也會顯示預設值。
- 評分邀請只會在你使用一段時間、看完 Bot 的成果並回到列表後出現，不會在開啟 App 時打擾你。也可以隨時從設定前往 App Store 評分。
- 「設定」現在是「狀態」右邊的獨立分頁，只放這支 iPhone 自己的設定。每台電腦的擴充套件、語音、螢幕、推送通知與連線，都移到「管理電腦」裡那台電腦的卡片上；推送通知可以逐台電腦開關。
- 電腦卡片最下方的「Codync」會顯示那台電腦的版本；有新版時可以直接點「更新」，從手機叫那台電腦更新。
- 電腦卡片多了網路按鈕（Beta）：一台電腦可以加入多個網路，這裡列出每個網路裡有哪些電腦、是否在線、各自提供哪些 Bot、誰可以傳訊息給這裡的 Bot，以及雲端助理是否開啟。網路的建立與邀請要在電腦上的 Codync 進行。
- Bot 設定多了「提供給網路」與「雲端助理可聯繫」兩個開關，決定其他電腦上的 Bot 與 ChatGPT、Claude 能不能找到它。

### en-US
- A context ring beside the message box shows your agent session’s usage; tap for token counts.
- Agent, model, reasoning effort and Fast mode now share compact, expandable settings. Each bot keeps its own effort preference, with the default level shown.
- Rating invitations wait until you have used Codync for a while, viewed a bot's result, and returned to the list. You can also rate Codync anytime from Settings.
- Settings is now its own tab, next to State, with only this iPhone's own settings. Each computer's Extensions, Voice chat, Screen, Push notifications and Connection are on its card in Manage computers, and push can be turned off for each computer.
- Codync at the bottom of a computer's card shows the version it runs; when a newer one is out, tap Update to have that computer update from your phone.
- A network button (beta) on each computer's card: a computer can be in several networks, and it shows each one's computers, whether each answers, the bots it offers, who may message the bots here, and whether cloud assistants are on. You set up and invite computers in Codync on a computer.
- Bot settings have Offered to the network and Reachable by cloud assistants, so you choose whether bots on your other computers, ChatGPT or Claude can find a bot.
