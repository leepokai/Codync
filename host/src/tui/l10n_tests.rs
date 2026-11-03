use super::{AppLanguage, HANS, PREFERENCE, language_title, resolved_language, tr, translate_with};
use crate::LockExt;

#[test]
fn brazilian_portuguese_round_trips_its_saved_identifier() {
    assert_eq!(AppLanguage::parse("pt-BR").stored(), "pt-BR");
    assert_eq!(AppLanguage::parse("pt").stored(), "system");
    assert_eq!(AppLanguage::parse("pt-PT").stored(), "system");
}

#[test]
fn portuguese_system_locales_use_the_brazilian_catalog() {
    for locale in [
        "pt",
        "pt-BR",
        "pt_BR.UTF-8",
        "pt-PT",
        "pt_AO.UTF-8",
        "pt-CV",
        "pt-GW",
        "pt-MO",
        "pt-MZ",
        "pt-ST",
        "pt-TL",
        "PT_br.utf8",
    ] {
        assert_eq!(resolved_language(AppLanguage::System, locale).stored(), "pt-BR", "{locale}");
    }
    let portuguese = AppLanguage::parse("pt-BR");
    assert_eq!(resolved_language(portuguese, "zh_TW").stored(), "pt-BR");
    assert_eq!(resolved_language(AppLanguage::En, "pt_BR.UTF-8"), AppLanguage::En);
}

#[test]
fn the_language_cycle_appends_portuguese_after_spanish() {
    let mut choice = AppLanguage::System;
    for identifier in ["zh", "zh-Hant", "en", "ja", "ko", "fr", "de", "es", "pt-BR", "ru", "system"] {
        choice = choice.next();
        assert_eq!(choice.stored(), identifier);
        assert_eq!(AppLanguage::parse(identifier), choice);
    }
}

#[test]
fn portuguese_selection_shows_its_native_title_and_loads_its_catalog() {
    let previous = {
        let mut choice = PREFERENCE.locked();
        let previous = *choice;
        *choice = AppLanguage::parse("pt-BR");
        previous
    };
    let title = language_title();
    let uses_portuguese_catalog =
        super::active_phrases().is_some_and(|table| std::ptr::eq(table, &raw const *super::PT_BR));
    let settings = tr("Settings");
    let unknown = tr("Unknown phrase");
    *PREFERENCE.locked() = previous;
    assert_eq!(title, "Português (Brasil)");
    assert!(uses_portuguese_catalog);
    assert_ne!(settings, "Settings");
    assert_eq!(unknown, "Unknown phrase");
}

#[test]
fn non_portuguese_system_locale_resolution_is_unchanged() {
    for (locale, expected) in [
        ("zh_CN.UTF-8", AppLanguage::Zh),
        ("zh_TW.UTF-8", AppLanguage::ZhHant),
        ("zh-Hant", AppLanguage::ZhHant),
        ("zh-HK", AppLanguage::ZhHant),
        ("zh_MO", AppLanguage::ZhHant),
        ("en_US.UTF-8", AppLanguage::En),
        ("ja_JP.UTF-8", AppLanguage::Ja),
        ("ko-KR", AppLanguage::Ko),
        ("fr_CA.UTF-8", AppLanguage::Fr),
        ("de-AT", AppLanguage::De),
        ("es_MX.UTF-8", AppLanguage::Es),
        ("ru_RU.UTF-8", AppLanguage::Ru),
        ("nl_NL.UTF-8", AppLanguage::En),
        ("C.UTF-8", AppLanguage::En),
        ("", AppLanguage::En),
    ] {
        assert_eq!(resolved_language(AppLanguage::System, locale), expected, "{locale}");
    }
}

#[test]
fn bot_routine_and_message_slots_keep_what_the_user_wrote() {
    assert_eq!(translate_with("Created bot connected", &HANS), "已创建机器人 connected");
    assert!(translate_with("Created bot Reviewer", &HANS).ends_with("Reviewer"));
    assert_eq!(translate_with("Routine · Settings", &HANS), "例行任务 · Settings");
    assert_eq!(translate_with("Message from Reviewer: Settings", &HANS), "来自 Reviewer 的消息：Settings");
    assert_eq!(translate_with("Runs the official installer on this Mac.", &HANS), "在 这台 Mac 上运行官方安装程序。");
}
