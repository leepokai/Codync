<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.32.0

### zh-Hant
- OpenAI 即時語音新增 GPT-Live 選項，可在交談時交辦工作、查詢進度與回覆審批。原有 Realtime 設定不變。
- GPT-Live 的模型測試會確認存取權限；語音與工具路由分別計費。

### en-US
- Choose GPT-Live for OpenAI voice calls while your bots handle tasks, status checks and approvals. Existing Realtime settings stay unchanged.
- GPT-Live model tests verify access without starting a paid call. Voice and tool routing are billed separately.
