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
- Codync 現在需要用 Apple、Google 或 GitHub 登入。以前沒登入就配對的電腦，第一次登入時會一起移進你的賬戶，不用再掃一次。
- 登出後，這支手機不再收到通知，但會保留它的電腦，重新登入不用再掃描。
- 可以在 App 裡刪除賬戶：點 Bots 分頁左上角的賬戶按鈕，再點「刪除賬戶」。
- 配對會確認賬戶：在別人賬戶裡的電腦會告訴你它在誰的賬戶；還不在任何賬戶的電腦，會先在電腦上問一聲，再加入你的賬戶。
- 也可以輸入 `codync-host pair` 印出的短代碼來配對，不用掃 QR code。
- 新增俄文介面：到「設定」選擇語言。

### en-US
- Sign in with Apple, Google or GitHub. Computers paired before come along, no new scan.
- Signing out stops notifications and keeps your computers: sign back in, no scan.
- Delete your account: account button (top left of Bots), then Delete account.
- Pairing checks accounts: a computer in another account says whose; one in none asks first, then joins yours.
- Or type the short code `codync-host pair` prints instead of scanning.
- New Russian interface: pick it under Settings → Language.
