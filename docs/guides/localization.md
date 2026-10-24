# Interface languages

Each device can choose Follow system, English, Simplified Chinese, Traditional
Chinese, Japanese, Korean, French, German or Spanish. Desktop and phone settings
hold the choice; the terminal command palette cycles its own saved preference.
Regional variants use their language's catalog. Unsupported system languages use
English. The saved `zh` identifier continues to mean Simplified Chinese.

The canonical UTF-8 JSON catalogs live in
`apps/ios/Kit/Sources/CodyncKit/Resources/Languages/`. iOS bundles them, desktop
imports them, the terminal embeds them, and Android includes that directory as
an asset source. Do not create a second Android copy.

English source phrases are the lookup keys. All catalogs must have identical
keys and preserve every `{placeholder}`. Missing phrases fall back to English;
Traditional Chinese retains its existing Simplified fallback. Translate an
interface template before inserting a user's name or text. Conversation content
is not translated. The desktop catalog test checks key and placeholder parity.

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
