<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.37.0
### zh-Hant
- 可在設定的語言選單選擇「Português (Brasil)」；跟隨系統時，葡萄牙語裝置會使用巴西葡萄牙語。
- GitHub 聯結器可以直接用 GitHub 帳號登入了：Codync 會顯示一組代碼，點「複製並開啟」，到 GitHub 頁面貼上並授權就完成，不用再自己建立 token。
- 之前用 token 加入 GitHub 的話，聯結器清單會提示「改用登入」，登入後就不用再管 token 過期。

### en-US
- Choose Português (Brasil) in Settings → Language, or follow your Portuguese system language.
- The GitHub connector now signs in with your GitHub account: Codync shows a short code, tap Copy and open, paste it on GitHub's page and authorize. No token to create.
- Added GitHub with a token before? Your connector list now offers to sign in instead, so the connector no longer breaks when the token expires.
