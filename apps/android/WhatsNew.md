<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.38.1
### zh-Hant
- bot 用了不只一種智慧體時，列表會在頭像左下角標出各自的智慧體標誌，可在「設定 › 外觀」關閉。
- 修正與電腦的連線中斷時，app 可能閃退的問題。
- 語音通話中語音辨識偶爾出錯時，會送出已聽到的內容並繼續聆聽，不再直接結束通話。
- 介面新增巴西葡萄牙文：可在設定選擇「Português (Brasil)」，或跟隨系統的葡萄牙文語言。
- 新增俄文介面：在設定選擇「Русский」，或跟隨系統的俄文語言。
- 用 QR Code 配對時，如果電腦當下沒能加入你的帳號（例如手機連不上 Codync 雲端，或電腦的雲端關著），下次連上時會自動補上。
- 語音通話的對話紀錄更好讀，通話列在 bot 沒說話時也會顯示一排靜止的點。

### en-US
- When bots use different agents, the bot list shows each one's logo (off in Settings › Appearance).
- Fixed a crash when the connection to your computer dropped.
- A one-off speech recognition error no longer ends a voice call.
- Choose Português (Brasil) or Русский in Settings, or follow your system language.
- A QR-paired computer that couldn't join your account joins it when it next connects.
- Calls read more clearly, and the call bar shows still dots while the bot is quiet.
