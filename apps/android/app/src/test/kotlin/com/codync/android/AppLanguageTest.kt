package com.codync.android

import com.codync.android.design.AppLanguage
import com.codync.android.design.translate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AppLanguageTest {
    @Test fun additionalLanguagesResolveRegionsAndKeepExplicitChoices() {
        for (language in listOf(AppLanguage.Ja, AppLanguage.Ko, AppLanguage.Fr, AppLanguage.De, AppLanguage.Es, AppLanguage.Ru, AppLanguage.It)) {
            assertEquals(language, AppLanguage.System.resolved(language.stored + "-XX"))
            assertEquals(language, language.resolved("zh-TW"))
            assertEquals(language, AppLanguage.fromStored(language.stored))
            assertEquals("translated", translate("Settings", language, false, mapOf("Settings" to "translated")))
            assertEquals("Unknown", translate("Unknown", language, true, phrases))
            assertFalse(language.usesChinese(true))
        }
    }

    @Test fun brazilianPortuguesePersistsAndAppearsAfterSpanish() {
        val language = AppLanguage.fromStored("pt-BR")
        assertEquals("pt-BR", AppLanguage.stored(language))
        assertEquals("Português (Brasil)", language.title)
        assertEquals(AppLanguage.Ru, AppLanguage.entries[AppLanguage.entries.indexOf(language) + 1])
        assertEquals(AppLanguage.Es, AppLanguage.entries[AppLanguage.entries.indexOf(language) - 1])
        assertEquals("Português (Brasil)", AppLanguage.menuTitle(language, AppLanguage.En, false, emptyMap()))
        assertEquals(language, language.resolved("zh-TW"))
        assertEquals(language, language.resolved("en-US"))
        assertFalse(language.usesChinese(true))
        assertEquals("Configurações", translate("Settings", language, false, mapOf("Settings" to "Configurações")))
        assertEquals("Unknown", translate("Unknown", language, false, emptyMap()))
    }

    @Test fun portugueseSystemLocalesUseBrazilianPortuguese() {
        val language = AppLanguage.fromStored("pt-BR")
        for (tag in listOf("pt", "pt-BR", "pt_BR.UTF-8", "PT-br", "pt-PT", "pt-AO", "pt-MZ", "pt-CV", "pt-GW", "pt-ST", "pt-TL", "pt-MO")) {
            assertEquals(tag, language, AppLanguage.System.resolved(tag))
        }
    }

    @Test fun existingSystemLocalesKeepTheirCatalogs() {
        val locales = mapOf(
            "en-US" to AppLanguage.En,
            "ru-RU" to AppLanguage.Ru,
            "it-IT" to AppLanguage.It,
            "it_CH.UTF-8" to AppLanguage.It,
            "zh-CN" to AppLanguage.Zh,
            "zh_Hans.UTF-8" to AppLanguage.Zh,
            "zh-TW" to AppLanguage.ZhHant,
            "zh-HK" to AppLanguage.ZhHant,
            "zh-MO" to AppLanguage.ZhHant,
            "zh-Hant" to AppLanguage.ZhHant,
        )
        for ((tag, language) in locales) {
            assertEquals(tag, language, AppLanguage.System.resolved(tag))
        }
    }

    @Test fun unsupportedLanguagesKeepTheirFallbacks() {
        for (tag in listOf("nl-NL", "unknown", "")) {
            assertEquals(tag, AppLanguage.En, AppLanguage.System.resolved(tag))
        }
        assertEquals(AppLanguage.System, AppLanguage.fromStored("unknown"))
        assertEquals(AppLanguage.System, AppLanguage.fromStored(null))
        assertEquals(AppLanguage.Zh, AppLanguage.fromStored("zh-Hans"))
    }

    private val phrases = mapOf("Pair a computer" to "配对一台电脑", "Follow system" to "跟随系统")

    @Test fun chineseFollowsTheChoiceNotTheOtherLanguage() {
        assertTrue(AppLanguage.Zh.usesChinese(systemIsChinese = false))
        assertTrue(AppLanguage.ZhHant.usesChinese(systemIsChinese = false))
        assertFalse(AppLanguage.En.usesChinese(systemIsChinese = true))
        assertTrue(AppLanguage.System.usesChinese(systemIsChinese = true))
        assertFalse(AppLanguage.System.usesChinese(systemIsChinese = false))
    }

    @Test fun phrasesStayEnglishUntilChineseIsChosen() {
        assertEquals("配对一台电脑", translate("Pair a computer", AppLanguage.Zh, systemIsChinese = false, phrases))
        assertEquals("Pair a computer", translate("Pair a computer", AppLanguage.En, systemIsChinese = true, phrases))
        assertEquals("跟随系统", AppLanguage.menuTitle(AppLanguage.System, AppLanguage.Zh, systemIsChinese = false, phrases))
        assertEquals("简体中文", AppLanguage.menuTitle(AppLanguage.Zh, AppLanguage.En, systemIsChinese = true, phrases))
        assertEquals("繁體中文", AppLanguage.menuTitle(AppLanguage.ZhHant, AppLanguage.En, systemIsChinese = true, phrases))
        assertEquals(AppLanguage.Zh, AppLanguage.fromStored("zh"))
        assertEquals(AppLanguage.ZhHant, AppLanguage.fromStored("zh-Hant"))
        assertEquals("Not a phrase", translate("Not a phrase", AppLanguage.Zh, systemIsChinese = false, phrases))
    }

    @Test fun aFilledSentenceReusesTheParsedTemplate() {
        val table = mapOf(
            "Runs the official installer on {name}." to "在 {name} 上运行官方安装程序。",
            "this Mac" to "这台 Mac",
        )
        val sentence = "Runs the official installer on this Mac."
        val expected = "在 this Mac 上运行官方安装程序。"
        assertEquals(expected, translate(sentence, AppLanguage.Zh, systemIsChinese = false, table))
        assertEquals(expected, translate(sentence, AppLanguage.Zh, systemIsChinese = false, table))
    }
    @Test fun filledTemplatesPreserveNamesMessagesAndNestedHints() {
        val table = mapOf(
            "Routine · {name}" to "例行任务 · {name}",
            "Message from {name}: {message}" to "来自 {name}：{message}",
            "{name} isn't set up. {hint}" to "{name} 尚未设置。{hint}",
            "Install {name}." to "安装 {name}。",
            "Plan" to "计划", "connected" to "已连接", "Standby" to "待命",
        )
        for (name in listOf("Plan", "connected", "Standby", "$&", "{hint}")) {
            assertEquals("例行任务 · $name", translate("Routine · $name", AppLanguage.Zh, false, table))
            assertEquals("来自 $name：Plan", translate("Message from $name: Plan", AppLanguage.Zh, false, table))
        }
        assertEquals("{hint} 尚未设置。安装 Plan。",
            translate("{hint} isn't set up. Install Plan.", AppLanguage.Zh, false, table))
    }
}
