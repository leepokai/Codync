<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.35.0
### zh-Hant
- Codync 現在需要用 Apple 或 Google 登入。以前沒登入就配對的電腦，第一次登入時會一起移進你的賬戶，不用再掃一次。
- 登出時，這支手機會從它連得到的電腦和你的賬戶中移除。
- 可以在 App 裡刪除賬戶：點 Bots 分頁左上角的賬戶按鈕，再點「刪除賬戶」。

### en-US
- Codync now asks you to sign in with Apple or Google. Computers you paired without signing in move into your account the first time you sign in, with no new scan.
- Signing out now also removes this phone from the computers it can reach and from your account.
- You can delete your account in the app: tap the account button at the top left of the Bots tab, then Delete account.
