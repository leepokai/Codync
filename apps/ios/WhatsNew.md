<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.25.0

### zh-Hant
- 新增「Open Live Mode」捷徑，選擇 Bot 後直接開啟語音聊天。
- 新增「Send Text to Bot」捷徑，可選擇 Bot 並傳送文字；預設在背景傳送，也可開啟「Open Chat」顯示對話。

### en-US
- Use Open Live Mode in Shortcuts to choose a bot and start voice chat.
- Use Send Text to Bot to choose a bot and send text in the background by default, or enable Open Chat to show the conversation.

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

### en-US
- Tap a message or empty conversation space to dismiss the keyboard while preserving scrolling and button actions
- Keep the latest message visible when opening the keyboard at the bottom of a chat
- The jump-to-latest button now uses native Liquid Glass

## 2.23.0

### zh-Hant
- 外觀可選擇跟隨系統、淺色或深色，並保留此裝置的偏好

### en-US
- Choose System, Light or Dark appearance and keep your preference on this device

## 2.22.2

### zh-Hant
- 擴充功能移除 Apps 區段，Composio 統一從 Connectors 登入使用

### en-US
- Extensions removes the Apps section; connect Composio from Connectors

## 2.22.1

### zh-Hant
- 已連線電腦的資訊與操作排列更緊湊，電腦名稱完整顯示
- 手機連不上帳號內的電腦時，可直接要求存取權限；連線說明也更容易閱讀
- 手機的裝置清單不再提供刪除同帳號電腦的操作，避免誤刪

### en-US
- Connected computer cards keep their full names visible with a more compact layout for connection details and actions
- Ask for access directly when your phone cannot reach an account computer, with clearer connection details
- Account computers can no longer be removed from the phone's device list, preventing accidental removal

## 2.22.0

### zh-Hant
- 授權卡片在寬螢幕上更精簡，不再撐滿聊天區
- 用麥克風聽寫時，麥克風按鈕會變成錄音膠囊：停止鍵、錄了多久，還有跟著你說話起伏的點點
- Bot 工作中會顯示簡潔的動畫、目前動作與經過時間，詳細紀錄可從「完整對話」查看
- 麥克風旁的向上箭頭可以選聽寫要用裝置內建語音，還是用你自己的 OpenAI 或 Gemini 金鑰
- iPad 接實體鍵盤時，在輸入框按 Return 會換行，⌘↩ 送出
- 群組裡被提及的 bot 若正在別的對話忙，群組會顯示它在忙，忙完就回來回答
- 記憶不會再滿：快滿時，Dreaming 會把細節依主題整理進筆記資料夾，你要 bot 記住的事一定存得下

### en-US
- Approval cards stay compact on wide screens instead of stretching across the chat
- While you dictate, the mic turns into a recording pill: a stop button, how long you've been talking, and dots that follow your voice
- While a bot works, see its activity and elapsed time in a compact animated indicator; details remain in Full conversation
- The arrow beside the mic picks how dictation hears you: on-device speech, or your own OpenAI or Gemini key
- On iPad with a hardware keyboard, Return starts a new line in the message box and ⌘↩ sends
- In a group, a bot you mention that's busy in another chat now says so, and answers once it's done
- Memory no longer fills up: as it nears its limit, dreaming files the detail into notes by topic, so what you ask a bot to remember always fits
