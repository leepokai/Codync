<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.36.0
### zh-Hant
- Codync 現在需要用 Apple 或 Google 登入。以前沒登入就配對的電腦，第一次登入時會一起移進你的賬戶，不用再掃一次。
- 登出時，這支手機會從它連得到的電腦和你的賬戶中移除。
- 可以在 App 裡刪除賬戶：點 Bots 分頁左上角的賬戶按鈕，再點「刪除賬戶」。
- 配對會確認賬戶：在別人賬戶裡的電腦會告訴你它在誰的賬戶；還不在任何賬戶的電腦，會先在電腦上問一聲，再加入你的賬戶。
- 也可以輸入 `codync-host pair` 印出的短代碼來配對，不用掃 QR code。

### en-US
- Codync now asks you to sign in with Apple or Google. Computers you paired before come along, no new scan.
- Signing out removes this phone from its computers and your account.
- Delete your account in the app: account button (top left of Bots), then Delete account.
- Pairing checks your account: a computer in another account says whose; one in no account asks first, then joins yours.
- Or type the short code `codync-host pair` prints instead of scanning.
