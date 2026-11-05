<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.39.1
### zh-Hant
- 推理強度滑桿新增漸層點陣，讓不同強度更容易辨識。

### en-US
- The reasoning effort slider now uses a dotted gradient to make levels easier to distinguish.

## 2.39.0
### zh-Hant
- 第一次設定時，如果帳號裡已有電腦，可以向它申請訪問，或配對一臺新電腦（例如 VPS）。安裝頁也能一鍵複製網站連結，到電腦上下載。
- bot 工作中送出的訊息會排在輸入框上方，可立即送出或收回。
- bot 用了不只一種智慧體時，列表會在頭像左下角標出各自的智慧體標誌，可在「設定 › 外觀」關閉。
- 修正與電腦的連線中斷時，app 可能閃退的問題。
- 語音通話中語音辨識偶爾出錯時，會送出已聽到的內容並繼續聆聽，不再直接結束通話。
- 介面新增巴西葡萄牙文：可在設定選擇「Português (Brasil)」，或跟隨系統的葡萄牙文語言。
- 新增俄文介面：在設定選擇「Русский」，或跟隨系統的俄文語言。
- 用 QR Code 配對的電腦如果當下沒能加入帳號，下次連上時會自動補上。
- 語音通話的對話紀錄更好讀，通話列在 bot 沒說話時也會顯示一排靜止的點。
- 排隊狀態、例行任務和執行紀錄會跟隨介面語言，名稱與訊息保留原文。
- 新增義大利文介面：可在設定選擇「Italiano」，或跟隨系統的義大利文語言。
- 擴充功能的「已安裝」提示也會顯示技能的圖示。

### en-US
- Messages sent while a bot works wait above the box: send now or take back.
- First-time setup offers your account's computers or a new one.
- When bots use different agents, the list shows each one's logo (off in Settings).
- Fixed a crash when the connection dropped.
- A one-off speech error no longer ends a voice call.
- Choose Português (Brasil), Русский or Italiano in Settings.
- A QR-paired computer joins your account when it next connects.
- Calls read more clearly.
