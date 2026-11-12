use super::{AppLanguage, HANS, PREFERENCE, fill, language_title, resolved_language, tr, translate, translate_with};
use crate::LockExt;

#[test]
fn what_fill_puts_in_stays_as_written() {
    assert_eq!(
        fill("{name} and {other}", &[("name", "Build {other}"), ("other", "Reviewer")]),
        "Build {other} and Reviewer"
    );
    assert_eq!(fill("{name} and {missing} {", &[("name", "Build")]), "Build and {missing} {");
}

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
    for identifier in ["zh", "zh-Hant", "en", "ja", "ko", "fr", "de", "es", "pt-BR", "ru", "it", "system"] {
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
}

#[test]
fn additional_languages_resolve_regions_and_load_catalogs() {
    for (language, table) in [
        (AppLanguage::Ja, &*super::JA),
        (AppLanguage::Ko, &*super::KO),
        (AppLanguage::Fr, &*super::FR),
        (AppLanguage::De, &*super::DE),
        (AppLanguage::Es, &*super::ES),
        (AppLanguage::Ru, &*super::RU),
        (AppLanguage::It, &*super::IT),
    ] {
        assert_eq!(resolved_language(AppLanguage::System, &format!("{}_XX.UTF-8", language.stored())), language);
        assert_eq!(resolved_language(language, "zh_TW"), language);
        assert_ne!(translate_with("Settings", table), "Settings");
        assert_eq!(translate_with("Unknown phrase", table), "Unknown phrase");
    }
    assert_eq!(resolved_language(AppLanguage::System, "nl_NL"), AppLanguage::En);
}

#[test]
fn chinese_replaces_a_known_phrase_and_leaves_the_rest() {
    assert_eq!(translate("Pair a computer", true).as_ref(), "配对一台电脑");
    assert_eq!(translate("Pair a computer", false).as_ref(), "Pair a computer");
    assert_eq!(translate("Not a phrase", true).as_ref(), "Not a phrase");
    assert_eq!(translate("Follow system", true).as_ref(), "跟随系统");
    assert_eq!(translate("Writing a reply…", true).as_ref(), "正在写回复…");
    assert_eq!(translate("Downloading Codebuddy Code…", true).as_ref(), "正在下载 Codebuddy Code…");
    assert_eq!(translate("Needs approval: Bash", true).as_ref(), "需要批准：Bash");
    assert_eq!(translate("Storage", true).as_ref(), "存储");
    assert_eq!(translate("New skill", true).as_ref(), "新建技能");
    assert_eq!(translate("Add connector", true).as_ref(), "添加连接器");
    assert_eq!(translate("Name your group", true).as_ref(), "给群聊起个名字");
    assert_eq!(translate("Nothing installed", true).as_ref(), "还没有安装");
    assert_eq!(translate("No setup needed.", true).as_ref(), "无需设置。");
    assert_eq!(
        translate("Runs the official installer on this Mac.", true).as_ref(),
        "在 this Mac 上运行官方安装程序。"
    );
    assert_eq!(
        translate("Claude Code isn't set up on this computer. Install Claude Code and run `claude auth login`.", true)
            .as_ref(),
        "Claude Code 还没有在这台电脑上设置好。安装 Claude Code，然后运行 `claude auth login`。"
    );
    assert_eq!(super::compact_time("3h 20m").as_deref(), Some("3小时20分"));
    assert_eq!(super::compact_time("2d 6h").as_deref(), Some("2天6小时"));
    assert_eq!(super::compact_time("Sep 26"), None);
}

#[test]
fn the_palette_label_reads_the_choice_without_locking_twice() {
    let title = language_title();
    assert!(!title.is_empty());
}

#[test]
fn a_missing_choice_follows_the_computer() {
    assert_eq!(AppLanguage::parse(""), AppLanguage::System);
    assert_eq!(AppLanguage::parse("zh"), AppLanguage::Zh);
    assert_eq!(AppLanguage::parse("zh-Hans"), AppLanguage::Zh);
    assert_eq!(AppLanguage::parse("zh-Hant"), AppLanguage::ZhHant);
    assert_eq!(AppLanguage::parse("en"), AppLanguage::En);
    assert_eq!(AppLanguage::System.next(), AppLanguage::Zh);
    assert_eq!(AppLanguage::Zh.next(), AppLanguage::ZhHant);
    assert_eq!(AppLanguage::ZhHant.next(), AppLanguage::En);
    assert_eq!(AppLanguage::En.next(), AppLanguage::Ja);
    assert_eq!(AppLanguage::Es.next(), AppLanguage::PtBr);
    assert_eq!(AppLanguage::PtBr.next(), AppLanguage::Ru);
    assert_eq!(AppLanguage::Ru.next(), AppLanguage::It);
    assert_eq!(AppLanguage::It.next(), AppLanguage::System);
    assert_eq!(super::system_script_of("zh_TW.UTF-8"), Some(super::ChineseScript::Hant));
    assert_eq!(super::system_script_of("zh-HK"), Some(super::ChineseScript::Hant));
    assert_eq!(super::system_script_of("zh_CN.UTF-8"), Some(super::ChineseScript::Hans));
    assert_eq!(super::system_script_of("en_US.UTF-8"), None);
    assert_eq!(translate_with("Settings", &super::HANT).as_ref(), "設定");
    assert_eq!(translate_with("Files", &super::HANT).as_ref(), "檔案");
}

#[test]
fn catalogs_preserve_names_and_substitute_captured_values_once() {
    for table in [
        &*HANS,
        &*super::HANT,
        &*super::JA,
        &*super::KO,
        &*super::FR,
        &*super::DE,
        &*super::ES,
        &*super::PT_BR,
        &*super::RU,
        &*super::IT,
    ] {
        for name in ["Plan", "connected", "Standby", "$&", "{hint}"] {
            let expected = translate_with("Routine · {name}", table).replace("{name}", name);
            assert_eq!(translate_with(&format!("Routine · {name}"), table), expected);
            let title = translate_with("Message about “{title}”", table).replace("{title}", name);
            assert_eq!(translate_with(&format!("Message about “{name}”"), table), title);
        }
        let hint = "Install Claude Code and run `claude auth login`.";
        let expected = translate_with("{name} isn't set up on this computer. {hint}", table)
            .replace("{hint}", &translate_with(hint, table))
            .replace("{name}", "{hint}");
        assert_eq!(translate_with(&format!("{{hint}} isn't set up on this computer. {hint}"), table), expected);
    }
}

#[test]
fn host_schedule_sentences_match_their_catalog_templates() {
    assert_eq!(
        translate_with("Every day at 09:00, 18:00 · Europe/Rome", &super::IT),
        "Ogni giorno alle 09:00, 18:00 · Europe/Rome"
    );
    assert_eq!(
        translate_with("Every Monday at 09:00 · Europe/Rome", &super::IT),
        "Ogni lunedì alle 09:00 · Europe/Rome"
    );
    assert_eq!(translate_with("When a webhook fires", &super::IT), "Quando arriva un webhook");
}
