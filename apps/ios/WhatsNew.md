<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.38.1
### zh-Hant
- 第一次設定時，如果帳號裡已有電腦，可以向它申請訪問，或配對一臺新電腦（例如 VPS）。安裝頁也能直接複製或分享下載連結到電腦。
- bot 用了不只一種智慧體時，列表會在頭像左下角標出各自的智慧體標誌，可在「設定 › 外觀」關閉。
- 修正語音通話中，bot 唸完回覆、要再聽你說話時 app 可能閃退的問題。
- 語音通話的對話紀錄更好讀：最上方顯示是跟哪個 bot、什麼時候、通話多久，語音說的話也更清楚。
- 通話列在 bot 沒說話時會顯示一排靜止的點，深色頭像的 bot 也看得到。
- 語音辨識出問題時，語音通話和聽寫會告訴你，不再一直停在聆聽或默默結束。
- 介面新增巴西葡萄牙文：可在設定選擇「Português (Brasil)」，或跟隨系統的葡萄牙文語言。
- 新增俄文介面：在設定選擇「Русский」，或跟隨系統的俄文語言。
- 用 QR Code 配對時，如果電腦當下沒能加入你的帳號（例如手機連不上 Codync 雲端，或電腦的雲端關著），下次連上時會自動補上。
- 排隊狀態、例行任務和執行紀錄會跟隨介面語言，名稱與訊息保留原文。
- 新增義大利文介面：可在設定選擇「Italiano」，或跟隨系統的義大利文語言。
- 擴充功能的「已安裝」提示也會顯示技能的圖示。

### en-US
- First-time setup now checks your account: ask a computer already in it for access, or pair a new one, like a VPS. The install page also has a download link to copy or share to your computer.
- When your bots use more than one agent, the bot list marks each avatar with its agent's logo. Turn it off in Settings › Appearance.
- Fixed a crash in voice calls when the bot finished reading a reply and started listening again.
- A call's conversation is easier to read: the bot, when the call ended and how long it lasted sit at the top, and the voice's words are clearer.
- The call bar shows a still row of dots while the bot isn't talking, for dark bots too.
- When speech recognition stops working, voice calls and dictation tell you, instead of listening forever or ending without a word.
- Choose Português (Brasil) in Settings, or follow your Portuguese system language.
- Choose Русский in Settings, or follow your Russian system language.
- Choose Italiano in Settings, or follow your Italian system language.
- The Installed button in Extensions also shows your skills' icons.
- A computer you pair with a QR code that couldn't join your account at the time (the phone couldn't reach the Codync cloud, or the computer's cloud was off, say) joins it the next time it connects.
- Queue status, routine notices and trace labels follow your language; names and messages stay as written.
