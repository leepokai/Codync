<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
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
