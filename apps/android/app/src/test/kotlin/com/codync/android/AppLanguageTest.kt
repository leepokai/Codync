package com.codync.android

import com.codync.android.design.AppLanguage
import com.codync.android.design.translate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AppLanguageTest {
    @Test fun additionalLanguagesResolveRegionsAndKeepExplicitChoices() {
        for (language in listOf(AppLanguage.Ja, AppLanguage.Ko, AppLanguage.Fr, AppLanguage.De, AppLanguage.Es)) {
            assertEquals(language, AppLanguage.System.resolved(language.stored + "-XX"))
            assertEquals(language, language.resolved("zh-TW"))
            assertEquals(language, AppLanguage.fromStored(language.stored))
            assertEquals("translated", translate("Settings", language, false, mapOf("Settings" to "translated")))
            assertEquals("Unknown", translate("Unknown", language, true, phrases))
            assertFalse(language.usesChinese(true))
        }
        assertEquals(AppLanguage.En, AppLanguage.System.resolved("pt-BR"))
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
        val expected = "在 这台 Mac 上运行官方安装程序。"
        assertEquals(expected, translate(sentence, AppLanguage.Zh, systemIsChinese = false, table))
        assertEquals(expected, translate(sentence, AppLanguage.Zh, systemIsChinese = false, table))
    }
}
