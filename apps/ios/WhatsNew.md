<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.29.0

### zh-Hant
- Agent、模型、推理強度與 Fast mode 整合成可收合的精簡設定；每個 Bot 可獨立保存推理強度，Default 也會顯示預設值。
- 評分邀請只會在你使用一段時間、看完 Bot 的成果並回到列表後出現，不會在開啟 App 時打擾你。也可以隨時從設定前往 App Store 評分。
- 「設定」現在是「狀態」右邊的獨立分頁，只放這支 iPhone 自己的設定。每台電腦的擴充套件、語音、螢幕、推送通知與連線，都移到「管理電腦」裡那台電腦的卡片上；推送通知可以逐台電腦開關。
- 電腦卡片最下方的「Codync」會顯示那台電腦的版本；有新版時可以直接點「更新」，從手機叫那台電腦更新。
- 電腦卡片多了網路按鈕（Beta）：一台電腦可以加入多個網路，這裡列出每個網路裡有哪些電腦、是否在線、各自提供哪些 Bot、誰可以傳訊息給這裡的 Bot，以及雲端助理是否開啟。網路的建立與邀請要在電腦上的 Codync 進行。
- Bot 設定多了「提供給網路」與「雲端助理可聯繫」兩個開關，決定其他電腦上的 Bot 與 ChatGPT、Claude 能不能找到它。

### en-US
- Agent, model, reasoning effort and Fast mode now share compact, expandable settings. Each bot keeps its own effort preference, with the default level shown.
- Rating invitations wait until you have used Codync for a while, viewed a bot's result, and returned to the list. You can also rate Codync anytime from Settings.
- Settings is now its own tab, next to State, with only this iPhone's own settings. Each computer's Extensions, Voice chat, Screen, Push notifications and Connection are on its card in Manage computers, and push can be turned off for each computer.
- Codync at the bottom of a computer's card shows the version it runs; when a newer one is out, tap Update to have that computer update from your phone.
- A network button (beta) on each computer's card: a computer can be in several networks, and it shows each one's computers, whether each answers, the bots it offers, who may message the bots here, and whether cloud assistants are on. You set up and invite computers in Codync on a computer.
- Bot settings have Offered to the network and Reachable by cloud assistants, so you choose whether bots on your other computers, ChatGPT or Claude can find a bot.

## 2.28.0

### zh-Hant
- 主助理的頭像右下角新增金色星星；點一下詳細頁的星星，即可了解它如何追蹤目標、協調 bots 與定時關心進度。
- 你的電腦連成一個網路後（在電腦上執行 codync-host network），Bot 可以請其他電腦上的 Bot 幫忙；聊天裡會寫出對方 Bot 和它在哪台電腦。
- 可以把 Codync 加進 ChatGPT 或 Claude 當聯結器，從那裡請你的 Bot 做事；Bot 的聊天會顯示「Message from Cloud assistant」。

### en-US
- Your lead agent now has a gold star on its avatar. Tap the larger star in its details to learn how it follows your goals, coordinates bots, and checks in.
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

## 2.26.0

### zh-Hant

- 配對電腦時可選擇 Windows，並查看安裝與掃描配對碼的步驟。
- 配對指引更清楚說明如何在兩台裝置使用相同的 Codync 或 Codync Dev 版本。
- 支援的 Antigravity 代理程式可回報各帳號與模型群組的方案用量。
- Bot 工作時，Gemini 與 OpenAI 的語音會主動告訴你它在做什麼、有沒有等你核准或遇到問題，做完就念出結果。
- 通話中短暫斷線再連上時，錯過的回覆會補念給你聽。
- Gemini 或 OpenAI 通話斷線時會自動重新連上並接著聊，不必重打；來電、Siri 或其他 App 佔用聲音時通話會暫停，結束後自動繼續，也可以點一下通話列下方繼續。
- 通話中，Bot 的即時動態會在鎖定畫面和動態島顯示通話時長和它正在做的事，還能直接靜音或結束通話，不用打開 App。
- Gemini 與 OpenAI 的語音用你正在說的語言回答，你換語言它就跟著換。

### en-US

- Choose Windows when pairing a computer and follow the installation and QR scanning steps.
- Clearer instructions for using matching Codync or Codync Dev apps on both devices when pairing.
- Supported Antigravity adapters report plan usage separately for each account and model group.
- While the bot works, the Gemini and OpenAI voice keeps you posted on what it's doing, tells you when an approval waits or something fails, and reads the result when it's done.
- A reply that arrives while a call briefly loses its connection is read once it's back.
- A Gemini or OpenAI call that loses its connection reconnects and carries on without hanging up; a phone call, Siri or another app's audio pauses the call, which picks up again when it ends, or with a tap under the call bar.
- During a call, the bot's Live Activity on the Lock Screen and in the Dynamic Island shows how long the call has run and what the bot is doing, with buttons to mute or end it without opening Codync.
- The Gemini and OpenAI voice answers in the language you speak and follows when you switch.

## 2.25.0

### zh-Hant

- 新增文件庫、Markdown 編輯與自動儲存，可審閱機器人提出的文件變更並查看電腦操作紀錄。
- 可依專案分組及篩選機器人，搜尋完整訊息紀錄，並預覽最近三則訊息而不標記已讀。
- 可在模型選單搜尋模型名稱與說明。
- 可為 Antigravity 機器人選擇獨立帳號。
- 機器人可透過安全卡片向你索取 API 金鑰或指令變數，值不會進入對話；可在「Credentials」中查看並移除。
- 改善遠端螢幕連線與例行工作通知的可靠性。
- 每台電腦都有一個主助理，固定在 Bot 列表最上面、帶著星星：它會主動跟進你在做的事，每天兩次看看有沒有該提醒你或能先幫你做的，需要時請你授權 Gmail、行事曆等服務；打電話給它時由它先開口。
- Bot 可以做 Artifact：報告、追蹤表、儀表板這類要回來看的內容會以卡片出現在聊天裡，點開看完整內容，直接在下方留言請它改，改好就自動更新。
- 每個 Bot 的「資料庫」收齊它做的 Artifact，以及聊天裡分享過的檔案和圖片。
- 可以建立使用 Hermes Agent 的 Bot。
- Bot 之間交辦工作不必等待，對方的回覆會回到發問的 Bot，再由它回覆你。
- 鎖定畫面的即時動態加上進度條，四格依任務進度填滿，顏色跟著狀態走。
- 帳號選單裡的「電腦與設定」和它打開的頁面同名。
- 新增聯結器失敗後再試一次，會使用你改好的金鑰。
- 使用 Hermes Agent 或 Gemini CLI 的 Bot，記憶頁會說明它們自己的記憶由所有用同一個 agent 的 Bot 共用，Codync 沒辦法關掉。
- 在設定的「對話」開啟「顯示思考細節」後，Bot 工作中點一下狀態膠囊，就能在對話裡展開這一輪的思考、工具與 MCP 呼叫，太長時在膠囊內捲動。
- 新增「Open Live Mode」捷徑，選擇 Bot 後直接開啟語音聊天；不選 Bot 就打給主助理，也可以對 Siri 說「Talk to Codync」。
- 新增「Send Text to Bot」捷徑，可選擇 Bot 並傳送文字；預設在背景傳送，也可開啟「Open Chat」顯示對話。
- 點對話上方的 Bot 會打開它的詳情、資料庫、記憶與設定；「新會話」改到 Bot 列表的長按選單。
- 用 Gemini 即時語音通話時，Gemini 聽得到你說話並會回答，也會念出 Bot 的回覆；先前通話可能一直沒有聲音、也沒有回應。
- 通話中在設定換了語音（例如從本機換成 Gemini），關掉設定後通話就改用新的語音繼續，不必掛斷重打。
- 語音通話連線中，通話列會顯示一條左右來回的線，真正連上、聽得到你說話後才換回音量點點。
- 有多台電腦時，Bot 列表中每台電腦的標題列都有自己的「擴充套件」按鈕，電腦的連線狀態也靠右對齊；擴充套件裡不再能切換電腦。
- 通話內容不再塞滿聊天：結束後只留一行，標題寫著這通電話在談什麼，點開可以看完整逐字稿；Bot 的記憶也會記住你在通話裡說的話。
- Gemini 與 OpenAI 的語音是 Bot 的轉接者：它知道你的記憶、Bot 的設定、專案現況和最近的對話，轉給 Bot 時附上你說的原話，也能停止 Bot、查 Bot 的記憶。

### en-US

- Keep Markdown documents with each bot, edit with autosave and conflict recovery, review proposed changes, and inspect computer activity.
- Filter bots, group them by project, search message history, and preview the last three messages without marking them read.
- Search model names and descriptions in the model picker.
- Choose a separate Antigravity account for each bot.
- Bots can ask for an API key or command variable on a secure card, and the value never enters the chat; review and remove them in Credentials.
- Improved remote screen connections and routine notification reliability.
- Every computer has a lead bot, starred on top of the Bots list: it follows up on what you're working on, checks in twice a day for anything worth a reminder or a head start, asks for access to services like Gmail or your calendar when it would help, and speaks first when you call it.
- Bots can make artifacts: reports, trackers and dashboards you come back to show as a card in the chat. Open one full screen and message the bot below it to change it; it updates in place.
- Each bot's Library collects its artifacts and the files and images shared in the chat.
- Create bots that run Hermes Agent.
- Bots hand work to each other without waiting; the answer comes back to the bot that asked, which then replies to you.
- Live Activities on the Lock Screen show a progress line that fills as the task moves, in the state's color.
- The account sheet's Computers & settings row now has the same name as the page it opens.
- Trying again after adding a connector failed uses the key you corrected.
- For bots that run Hermes Agent or Gemini CLI, Memory explains that the agent's own memory is shared by every bot that runs it and Codync can't turn it off.
- Turn on Show thinking details in Settings › Chat, then tap a working bot's status pill to unfold this turn's thinking, tool and MCP calls right in the chat; a long turn scrolls inside the pill.
- Use Open Live Mode in Shortcuts to choose a bot and start voice chat; leave the bot out to call your lead bot, or say "Talk to Codync" to Siri.
- Use Send Text to Bot to choose a bot and send text in the background by default, or enable Open Chat to show the conversation.
- Tapping the bot at the top of its chat opens its Details, Library, Memory and Settings; New session moved to the bot's touch-and-hold menu in the list.
- Gemini realtime voice calls hear you, answer and read the bot's replies aloud; before, a call could stay silent with no reply.
- Pick another voice in the call settings mid-call (on-device to Gemini, say) and the call carries on with it once you close them.
- While a voice call connects, the call bar shows a line gliding back and forth until the voice can really hear you.
- With several computers, each computer's heading in the Bots list has its own Extensions button, with its connection status on the right; Extensions no longer switches computers.
- Calls no longer fill the chat: each one leaves a single line titled with what it was about; tap it for the full transcript. The bot's memory keeps what you said on the call.
- The Gemini and OpenAI voice relays to your bot: it knows your memory, the bot's setup, the project and the recent conversation, passes your own words along, and can stop the bot or look up what it remembers.

## 2.24.0

### zh-Hant
- 設定與語音聊天的標題加入清楚的圖示。
- 可在設定中自行選擇英文、簡體中文、繁體中文、日語、韓語、法語、德語或西班牙語，也可跟隨系統語言。
- 語言偏好保留在這台裝置，聊天內容維持原文。

### en-US
- Identify Settings and Voice chat with clearer header icons.
- Choose English, Simplified Chinese, Traditional Chinese, Japanese, Korean, French, German or Spanish in Settings, or follow your system language.
- Keep your language preference on this device without translating your conversations.

## 2.23.1

### zh-Hant
- 點一下聊天訊息或空白區域即可收起鍵盤，保留正常捲動與按鈕操作
- 聊天停在底部時，鍵盤開啟後仍保持最後一則訊息可見
- 回到最新訊息按鈕改用系統 Liquid Glass
