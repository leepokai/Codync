<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.28.0

### zh-Hant
- 你的電腦連成一個網路後（在電腦上執行 codync-host network），Bot 可以請其他電腦上的 Bot 幫忙；聊天裡會寫出對方 Bot 和它在哪台電腦。
- 可以把 Codync 加進 ChatGPT 或 Claude 當聯結器，從那裡請你的 Bot 做事；Bot 的聊天會顯示「Message from Cloud assistant」。

### en-US
- Once your computers share a network (codync-host network on each), a bot can ask a bot on another computer; the chat names that bot and its computer.
- Add Codync to ChatGPT or Claude as a connector to give your bots tasks from there; the bot's chat shows Message from Cloud assistant.

## 2.27.0

### zh-Hant
- 此版本起需要 iOS 26 或更新版本。
- 修正送出文字、清空或替換聊天草稿時可能閃退的問題。
- 擴充套件的列表改回往下捲動，不再分頁；聯結器捲到底會自動從 MCP Registry 載入更多。
- 點聯結器或智慧體只會打開詳情，要按「Add」或「Set up」才會加入或下載。
- 用量也會顯示這台電腦上其他 agent 的額度，包括 oh-my-pi、Amp、Kiro、Kimi、Antigravity、OpenCode、Mistral Vibe 和 Junie。Cursor 會用本機已登入的帳號向 cursor.com 查詢用量。

### en-US
- This version requires iOS 26 or later.
- Fixed a possible crash when sending text or clearing or replacing a chat draft.
- Extensions lists scroll again instead of paging; connectors load more from the MCP Registry as you reach the end.
- Tapping a connector or agent only opens its details; nothing is added or downloaded until you press Add or Set up.
- Usage limits now include the other agents installed on your computer: oh-my-pi, Amp, Kiro, Kimi, Antigravity, OpenCode, Mistral Vibe and Junie. Cursor asks cursor.com with the sign-in saved on that computer.

## 2.26.2

### zh-Hant
- Bot 傳來的圖片會直接顯示在聊天裡，點一下可以放大看，不用先下載。
- 電腦上的 Codync 版本太舊時，更新提示多了「Update now」按鈕，直接從手機叫那台電腦更新。
- 擴充套件一次顯示一個分類：智慧體、聯結器或技能，每個列表都分頁。點列表兩側的圓形按鈕或左右滑動就能翻頁，下方可選每頁 6、10、20 或 50 筆，並顯示目前在第幾頁。
- 聯結器翻到還沒載入的頁面時，會從 MCP Registry 接著載入。
- 「憑證」移到上方電腦名稱旁；「自定義聯結器」和「自己的技能」放在各自分類的列表上方。
- 電腦使用自建中繼時，設定和連線狀態會顯示該中繼的地址，不再一律寫成 Cloudflare。
- 可以建立使用 WorkBuddy 的 Bot：Codync 會使用 WorkBuddy 桌面 App 內附的 CLI，並在聊天裡請你登入。

### en-US
- Images a bot sends now show right in the chat; tap one to see it full size, no download needed.
- When a computer's Codync is too old for this app, Update now on the notice updates that computer from your phone.
- Extensions shows one section at a time: Agents, Connectors or Skills, each in pages. Tap the round buttons beside the list or swipe left or right to turn the page; under it, pick 6, 10, 20 or 50 per page and see which page you're on.
- Connectors keep loading from the MCP Registry as you page past what's loaded.
- Credentials sits beside the computer's name at the top, and Custom connector and Your own skill sit above their lists.
- When a computer uses a private relay, Settings and connection status show that relay's address instead of always saying Cloudflare.
- Create bots that run WorkBuddy: Codync uses the CLI inside the WorkBuddy desktop app and asks you to sign in from the chat.

## 2.26.1

### zh-Hant
- 在背景從即時動態或鎖定畫面掛斷通話時，聊天裡也會留下這通電話的紀錄，回到 App 不再顯示連不上電腦。

### en-US
- A call you end from its Live Activity or the Lock Screen, with the app in the background, still leaves its line in the chat, and coming back no longer says your computer can't be reached.
