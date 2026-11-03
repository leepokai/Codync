<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.38.1
### zh-Hant
- 修正語音通話中，bot 唸完回覆、要再聽你說話時 app 可能閃退的問題。
- 介面新增巴西葡萄牙文：可在設定選擇「Português (Brasil)」，或跟隨系統的葡萄牙文語言。
- 新增俄文介面：在設定選擇「Русский」，或跟隨系統的俄文語言。
- 用 QR Code 配對時，如果電腦當下沒能加入你的帳號（例如手機連不上 Codync 雲端，或電腦的雲端關著），下次連上時會自動補上。

### en-US
- Fixed a crash in voice calls when the bot finished reading a reply and started listening again.
- Choose Português (Brasil) in Settings, or follow your Portuguese system language.
- Choose Русский in Settings, or follow your Russian system language.
- A computer you pair with a QR code that couldn't join your account at the time (the phone couldn't reach the Codync cloud, or the computer's cloud was off, say) joins it the next time it connects.
