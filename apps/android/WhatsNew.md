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
- 可以從訊息選單刪除任何訊息；它會在所有裝置上消失。
- 每個 Bot 的聊天可以選擇顏色，方便一眼分辨。
- Slack、Telegram 與 Discord 的提及可以啟動流程；GitHub 的 issue、PR 與失敗檢查也一樣。
- 新增 TickTick 等應用程式，以及每個代理程式的建立者、審核狀態、分類與更新日期。
- WhatsApp 號碼可以用配對代碼連結，不必掃描 QR 碼。
- 中斷的任務與失敗的例行工作可以用「取消任務」結束，訊息會保留。
- 語音通話的停頓改成滑桿（0.6–3 秒），朗讀速度可選 0.5 倍到 2 倍。
- Bot 頭像可選絨感或扁平，靜止時的表情更放鬆，群組頭像會輕輕浮動。
- Proton Pass 加入目錄：填入個人存取權杖後，Bot 即可讀取它開啟的保險庫。
- Composio 的應用程式在擴充功能中列出並顯示連線狀態，捲動時會繼續載入。
- 被用完的方案額度擋住的模型會說明原因，以及額度恢復的時間。
- 長按對話可以標為未讀；重新開啟後會出現「新訊息」分隔線，對話的媒體與檔案也會列出。
- 連線不穩或系統清掉 App 之後會自行重新連上，省電模式也不再中斷串流中的回覆。
- 設定新增「支援」：Discord、GitHub 與可翻頁的支援記錄，記錄可以清除。
- 例行工作可以由 WhatsApp 訊息或群組提及啟動，也可以把 WhatsApp 對話當作結果。
- 流程可以接手 Slack 觸發條件與結果。
- 等待中的訊息會列在意見欄上方，可以改寫、直接插進目前的回覆，或刪除。
- 每段文字依自己的方向閱讀，程式碼一律由左至右；介面可以選阿拉伯文並由右至左排列。
- 可以用 PIN 或手機的指紋、臉部、螢幕鎖定鎖住 App，離開後依設定重新鎖上。
- 從其他 App 分享時，分享面板會直接列出釘選的代理程式。
- 頻道畫面會顯示這台電腦的 Telegram 機器人、Discord、Slack 與 GitHub；可以替機器人配對對象或把人移除。
- WhatsApp 現在有自己的一列，從擴充功能市集也能開啟連結畫面。
- 工作流程的連線可以指定目標機器人那一輪要用的模型與選項。
- 每個聊天都可以有討論串：從任何訊息或列表開啟，回覆留在討論串裡，聊天只顯示回覆數。
- 語音通話新增 ElevenLabs：使用你自己的金鑰，由 Scribe 聽你說話、帳號中的聲音讀出回覆；測試會唸出「OK」。
- Hermes 環境、記憶與子代理路由可一次套用。
- 通話可選 NeuTTS 本機串流語音，說話可中斷回覆；手機新增自選 Hey Hermes 前景喚醒。
- 擴充功能加入 Hermes 技能與 MCP 應用程式，標示來源；技能可離線安裝。
- Bot 可保留自己的技能改進，並依要求查看、還原版本。
- Telegram、WhatsApp、Slack、Discord 語音訊息沿用語音金鑰；例行工作可透過已連結的 WhatsApp 傳送報告。
- 市集的代理附技能與例行工作；Workflow Builder 可建立連動步驟。
- Bot 可使用獨立瀏覽器設定檔與聊天顏色；訊息刪除會同步。
- Slack、Telegram、Discord 與 GitHub 事件可啟動流程；新增 TickTick 等應用程式及代理資訊。

### en-US
- Extensions is now a marketplace with Apps, Agents and Skills, featured picks and categories.
- Adding an agent creates a bot with its instructions, look, skills and routines; read its page first.
- Bundled skills install offline, and catalogued apps add their connector in one step.
- The new Workflow Builder turns a trigger, steps and a result into a workflow that draws itself on the canvas.
- Each bot can name a browser profile, keeping sign-ins and cookies inside that profile.
- Any message can be deleted from its menu and disappears on every device.
- Each bot's chat can have its own color, so it is easy to tell apart.
- Slack, Telegram and Discord mentions can start a workflow, as can GitHub issues, pull requests and failed checks.
- New apps such as TickTick, and each agent page now names its creator, review, category and update date.
- A WhatsApp number can be linked by typing a pairing code instead of scanning a QR code.
- A task that stopped halfway, or a routine run that failed, can be ended with Cancel task; its messages stay.
- The voice call pause is a slider from 0.6 to 3 seconds, and read-aloud speed goes from 0.5× to 2×.
- Bot avatars can be plush or flat, resting faces look calmer, and group avatars drift gently.
- Proton Pass joins the catalog: add its personal access token and your bots can read the vault it opens.
- Composio's apps are listed in Extensions with their connection status and load as you scroll.
- A model stopped by a spent plan limit says so, and when the limit comes back.
- Long-press a chat to mark it unread; opening it again shows a New messages line, and the chat lists its media and files.
- Codync reconnects on its own after a flaky connection or the system stopping it, and battery saver no longer cuts a streaming reply.
- Settings has Support: Discord, GitHub and a paged support log you can clear.
- A routine can start from a WhatsApp message or group mention, and a WhatsApp chat can be a routine output.
- Workflows accept Slack triggers and outputs.
- Waiting messages show above the composer to rewrite, steer into the running reply, or delete.
- Each paragraph reads in its own direction and code always reads left to right; the interface comes in Arabic, right to left.
- Lock Codync behind a PIN or the phone's fingerprint, face or screen lock, and it locks again after the chosen time away.
- Sharing from another app offers your pinned agents in the share sheet.
- Channels shows the computer's Telegram bots, Discord, Slack and GitHub from this phone; pair someone to a bot or take them out.
- WhatsApp has its own row in Extensions, and the linked number stays reachable from computer management.
- A workflow link can name the model and its options for the target bot's turn.
- Every chat can have threads: open one from any message or the list; replies stay in the thread and the chat shows a "3 replies" chip.
- Voice calls add ElevenLabs: on your own key, Scribe hears you and the account's own voices read replies; Test says "OK".
- Apply Hermes environment, memory and delegation routes together.
- Local NeuTTS streams replies; talk to interrupt. Optional foreground Hey Hermes.
- Extensions adds offline Hermes skills, MCP apps and restorable skill improvements.
- Voice notes reuse speech keys; WhatsApp delivers routine reports.
- Marketplace agents include skills and routines; Workflow Builder links steps.
- Browser profiles, chat colors and synced deletion. Event workflows and TickTick.

## 2.32.0

### zh-Hant
- Bot 清單可以分成自己的分組，例如「工作」和「個人」：長按 Bot 選「移到分組」，點分組標題即可收合，收合時會顯示裡面的未讀數。
- 遠端畫面連線更快，網路短暫中斷時也不會斷掉。

### en-US
- Sort the bot list into your own sections, like Work and Personal: touch and hold a bot and choose Move to section, then tap a section's heading to fold it, with its unread count showing while it's folded.
- The remote screen connects faster and stays up through a brief connection drop.
