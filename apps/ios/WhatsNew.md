<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.39.0

### zh-Hant
- 「擴充功能」成為市集：應用程式、代理與技能三個分頁，加上精選與分類。
- 加入代理會建立一個帶指示、外觀、技能與例行工作的 Bot；先看它的頁面再決定。
- 內建的技能可離線安裝，目錄中的應用程式一步接好連接器。
- 新的工作流程編輯器把觸發、步驟與結果畫成一張會自己展開的畫布。
- 每個 Bot 可以指定瀏覽器設定檔，登入與 Cookie 都留在該設定檔裡。
- Slack、Telegram 與 Discord 的提及、GitHub 的 issue、PR 與失敗檢查都能啟動工作流程；WhatsApp 訊息也能。
- 任何訊息都能從選單刪除，並在所有裝置上消失。
- 每個 Bot 的聊天可以有各自的顏色，方便分辨。
- WhatsApp 可以輸入配對碼連結，不必掃 QR 碼；也能回答核准並限制誰能寫入。
- 中斷或失敗的任務、失敗的例行工作可以用「取消任務」結束，訊息會保留。
- Bot 頭像可選蓬鬆或平面，靜止表情更平靜，群組頭像會輕輕漂移。
- Proton Pass 進入目錄：貼上個人存取權杖，Bot 就能讀取它開啟的保險箱。
- 被用完的額度會說明原因，並在額度回來時提示。
- 使用量頁面有「你的一週」，也能挑選背景工作用的模型並切換省電。
- Bot 可以被要求在說完成前先檢查，也能設定它的個性。
- 長按聊天可標為未讀；再次打開會看到「新訊息」線，聊天也會列出它的媒體與檔案。
- 連線不穩或系統中止後 Codync 會自己重連，省電模式不再中斷串流回覆。
- 設定新增「支援」：Discord、GitHub，以及可分頁、可清除的支援記錄。
- 例行工作可以從 WhatsApp 訊息或群組提及開始，也能把 WhatsApp 聊天當成輸出的去處。
- 工作流程接受 Slack 觸發與輸出。
- 介面新增阿拉伯文，並把整個版面鏡像；每段文字仍依自己的方向閱讀。
- 佇列訊息顯示在輸入框上方，可先編輯、插話或刪除。
- 例行工作可以選擇在子討論串中執行。
- 頻道畫面會顯示這台電腦的 Telegram 機器人、Discord、Slack 與 GitHub；可以替機器人配對對象或把人移除。
- WhatsApp 現在有自己的一列，從電腦管理或擴充功能市集都能開啟連結畫面。
- 工作流程的連線可以指定目標機器人那一輪要用的模型與選項。
- 每個聊天都可以有討論串：從任何訊息或列表開啟，回覆留在討論串裡，聊天只顯示「3 則回覆」。
- 語音通話新增 ElevenLabs：用你自己的金鑰，由 Scribe 聽你說話、帳號裡的聲音讀出回覆；測試會唸出「OK」。

### en-US
- Extensions is now a marketplace with Apps, Agents and Skills, featured picks and categories.
- Adding an agent creates a bot with its instructions, look, skills and routines; read its page first.
- Bundled skills install offline, and catalogued apps add their connector in one step.
- The new Workflow Builder turns a trigger, steps and a result into a workflow that draws itself on the canvas.
- Each bot can name a browser profile, keeping sign-ins and cookies inside that profile.
- Slack, Telegram and Discord mentions can start a workflow, as can GitHub issues, pull requests and failed checks, and WhatsApp messages.
- Any message can be deleted from its menu and disappears on every device.
- Each bot's chat can have its own color, so it is easy to tell apart.
- A WhatsApp number can be linked by typing a pairing code instead of scanning a QR code; approvals can be answered and who writes restricted.
- A task that stopped halfway, or a routine run that failed, can be ended with Cancel task; its messages stay.
- Bot avatars can be plush or flat, resting faces look calmer, and group avatars drift gently.
- Proton Pass joins the catalog: add its personal access token and your bots can read the vault it opens.
- A model stopped by a spent plan limit says so, and when the limit comes back.
- Usage shows your week, and a model can be picked for background work with a Saver switch.
- A bot can be asked to check its work before saying it is done, and its character can be set.
- Long-press a chat to mark it unread; opening it again shows a New messages line, and the chat lists its media and files.
- Codync reconnects on its own after a flaky connection or the system stopping it, and battery saver no longer cuts a streaming reply.
- Settings has Support: Discord, GitHub and a paged support log you can clear.
- A routine can start from a WhatsApp message or group mention, and a WhatsApp chat can be a routine output.
- Workflows accept Slack triggers and outputs.
- The interface is available in Arabic and mirrors right to left, while each paragraph still reads in its own direction.
- Queued messages sit above the composer, ready to edit, steer or delete.
- A routine can run in a thread.
- Channels shows the computer's Telegram bots, Discord, Slack and GitHub from this iPhone; pair someone to a bot or take them out.
- WhatsApp has its own row in computer management, beside its row in the marketplace.
- A workflow link can name the model and its options for the target bot's turn.
- Every chat can have threads: open one from any message or the list; replies stay in the thread and the chat shows a "3 replies" chip.
- Voice calls add ElevenLabs: on your own key, Scribe hears you and the account's own voices read replies; Test says "OK".
