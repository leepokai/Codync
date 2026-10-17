package com.codync.android.core

import java.util.Locale
import org.junit.Assert.*
import org.junit.Test

class VoiceLanguagesTest {
    private val onDevice = listOf("en-US", "zh-CN", "cmn-Hant-TW", "ja-JP", "fr-FR")

    @Test fun followingThePhoneUsesItsLanguageWhenAndroidHearsItElseEnglish() {
        assertEquals("cmn-Hant-TW", VoiceLanguages.resolve("", Locale.forLanguageTag("zh-TW"), onDevice))
        assertEquals("zh-CN", VoiceLanguages.resolve("", Locale.forLanguageTag("zh-SG"), onDevice))
        assertEquals("fr-FR", VoiceLanguages.resolve("", Locale.forLanguageTag("fr-CA"), onDevice))
        assertEquals("en-US", VoiceLanguages.resolve("", Locale.forLanguageTag("de-DE"), onDevice))
        assertEquals("en-US", VoiceLanguages.resolve("", Locale.forLanguageTag("zh-TW"), emptyList()))
        assertEquals("ja-JP", VoiceLanguages.resolve("ja-JP", Locale.US, onDevice))
    }

    @Test fun androidHearsItsLanguagesAndTheBundledModelOnlyEnglish() {
        assertEquals(Recognition.Platform, VoiceLanguages.recognition("ja-JP", onDevice))
        assertEquals(Recognition.Platform, VoiceLanguages.recognition("en-US", onDevice))
        assertEquals(Recognition.Bundled, VoiceLanguages.recognition("en-US", emptyList()))
        assertEquals(Recognition.Bundled, VoiceLanguages.recognition("en-GB", onDevice))
        assertNull(VoiceLanguages.recognition("de-DE", onDevice))
    }

    @Test fun repliesAreReadInTheLanguageTheyAreWrittenIn() {
        assertEquals("cmn-Hant-TW", VoiceLanguages.spoken("好的，我已經修好了。", "cmn-Hant-TW"))
        assertEquals("zh", VoiceLanguages.spoken("好的，測試都通過了 OK", "en-US"))
        assertEquals("ja-JP", VoiceLanguages.spoken("テストは全部通りました", "en-US"))
        assertEquals("ko-KR", VoiceLanguages.spoken("테스트가 통과했습니다", "en-US"))
        assertEquals("en-US", VoiceLanguages.spoken("All tests pass.", "cmn-Hant-TW"))
        assertEquals("fr-FR", VoiceLanguages.spoken("Tous les tests passent.", "fr-FR"))
    }

    @Test fun recognizedStretchesJoinWithSpacesOnlyBetweenWords() {
        assertEquals("run the tests", VoiceLanguages.join("run the", "tests"))
        assertEquals("幫我跑測試", VoiceLanguages.join("幫我", "跑測試"))
        assertEquals("tests", VoiceLanguages.join("", "tests"))
        assertEquals("run", VoiceLanguages.join("run", ""))
    }
}
