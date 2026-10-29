<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.32.0

### zh-Hant
- OpenAI 即時語音新增 GPT-Live 選項，可在交談時交辦工作、查詢進度與回覆審批。原有 Realtime 設定不變。
- GPT-Live 的模型測試會確認存取權限；語音與工具路由分別計費。

### en-US
- Choose GPT-Live for OpenAI voice calls while your bots handle tasks, status checks and approvals. Existing Realtime settings stay unchanged.
- GPT-Live model tests verify access without starting a paid call. Voice and tool routing are billed separately.
