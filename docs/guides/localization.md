# Interface languages

Each device can choose Follow system, English, Simplified Chinese, Traditional
Chinese, Japanese, Korean, French, German, Spanish, Brazilian Portuguese, Russian or Italian.
Desktop and phone settings hold the choice; the terminal command palette cycles
its own saved preference. Regional variants use their language's catalog.
Portuguese system tags (`pt`, `pt-BR`, `pt_BR.UTF-8`, `pt-PT`) use `pt-BR`, shown
as **Português (Brasil)** after Spanish. Unsupported system languages use English.
The saved `zh` identifier continues to mean Simplified Chinese.

The canonical UTF-8 JSON catalogs live in
`apps/ios/Kit/Sources/CodyncKit/Resources/Languages/`. iOS bundles them, desktop
imports them, the terminal embeds them, and Android includes that directory as
an asset source. Do not create a second Android copy.

English source phrases are the lookup keys. All catalogs must have identical
keys and preserve every `{placeholder}`. Missing phrases fall back to English;
Traditional Chinese retains its existing Simplified fallback. Translate an
interface template before inserting a user's name or text. Conversation content
is not translated. The desktop catalog test checks key and placeholder parity.

A captured name, title, query, sign-in code or message is opaque, even when it
matches an English interface phrase. Only app-owned `{hint}`, `{error}`,
`{setup}`, `{kind}`, `{when}`, `{status}`, `{reason}`, `{shortcut}` and `{changes}` slots
translate recursively. Cloud errors use `{reason}` and `{hint}` for their
interface text; keyboard hints use `{shortcut}`. Substitute captured values in
one pass so a name containing another placeholder's spelling stays literal.

Russian system locales (including `ru-RU` and `ru_RU.UTF-8`) use `ru`, shown
as **Русский** after Brazilian Portuguese. The Russian catalog uses a fixed
glossary (бот, ведущий агент, рутина, коннектор) and count-neutral phrases such
as «Ботов: {count}» so every number remains grammatical.

Italian system locales (including `it-IT` and `it_CH.UTF-8`) use `it`, shown
as **Italiano** after Russian. The catalog began as a machine translation and
every entry was then corrected against a fixed glossary (bot, agente, agente
principale, routine, connettore); no native speaker has reviewed it yet.

The five additional catalogs began as machine translations of public interface
copy, with reviewed overrides for common controls. They have not had a complete
native-speaker review. Translation improvements should change the canonical
catalog and run the catalog test, together with the affected client checks.

For a new language, add its catalog and register its identifier/native name in
Swift `AppLanguage`, desktop `appLanguages`, Android `AppLanguage`, and terminal
`AppLanguage`. Add it to the Swift package resources, desktop imports, and Rust
embedded catalogs; Android picks up the shared JSON automatically. Test explicit
selection, persistence, regional system tags, unknown-language fallback, widgets,
and the desktop tray. Update the iPhone release notes.
