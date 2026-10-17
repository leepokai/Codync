package com.codync.android.core

import java.util.Locale

/** How an on-device call hears its language: Android's on-device recognizer, or the bundled English model. */
enum class Recognition { Platform, Bundled }

/**
 * The languages of an on-device call (iOS `OnDeviceEngine.languageOptions`), as BCP 47 tags; an empty
 * pick follows the phone. `onDevice` is what Android's on-device recognizer can hear on this phone.
 */
object VoiceLanguages {
    const val ENGLISH = "en-US"

    /** The call's language: the picked one, else the phone's when Android hears it on device, else English. */
    fun resolve(picked: String, system: Locale, onDevice: Collection<String>): String =
        picked.ifEmpty { match(system, onDevice) ?: ENGLISH }

    /** Android's closest on-device language for a locale: the same language and region, then the language. */
    fun match(locale: Locale, onDevice: Collection<String>): String? {
        val tags = onDevice.map { it to Locale.forLanguageTag(it) }
        val language = base(locale)
        return tags.firstOrNull { (_, it) -> base(it) == language && it.country == locale.country }?.first
            ?: tags.firstOrNull { (_, it) -> base(it) == language }?.first
    }

    /** Who hears the language, or null when nothing on this phone can. */
    fun recognition(tag: String, onDevice: Collection<String>): Recognition? = when {
        tag in onDevice -> Recognition.Platform
        base(Locale.forLanguageTag(tag)) == "en" -> Recognition.Bundled
        else -> null
    }

    /**
     * The language to read a reply in, from its writing: Japanese, Korean and Chinese by script; other
     * writing follows the call's language, or English when the call is in one of those three.
     */
    fun spoken(text: String, call: String): String {
        var han = 0; var kana = 0; var hangul = 0; var other = 0
        for (char in text) when (Character.UnicodeScript.of(char.code)) {
            Character.UnicodeScript.HAN -> han++
            Character.UnicodeScript.HIRAGANA, Character.UnicodeScript.KATAKANA -> kana++
            Character.UnicodeScript.HANGUL -> hangul++
            else -> if (char.isLetter()) other++
        }
        val language = base(Locale.forLanguageTag(call))
        return when {
            kana > 0 && kana + han >= other -> if (language == "ja") call else "ja-JP"
            hangul > 0 && hangul >= han + other -> if (language == "ko") call else "ko-KR"
            han > 0 && han >= other -> if (language == "zh") call else "zh"
            language in listOf("zh", "ja", "ko") -> ENGLISH
            else -> call
        }
    }

    /** Joins recognized stretches: no space between Chinese, Japanese or Korean characters. */
    fun join(heard: String, next: String): String = when {
        heard.isEmpty() -> next
        next.isEmpty() -> heard
        wide(heard.last()) || wide(next.first()) -> heard + next
        else -> "$heard $next"
    }

    /** The language part of a locale, with Mandarin's `cmn` read as `zh`. */
    fun base(locale: Locale): String = locale.language.let { if (it == "cmn") "zh" else it }

    private fun wide(char: Char) = Character.UnicodeScript.of(char.code) in setOf(Character.UnicodeScript.HAN,
        Character.UnicodeScript.HIRAGANA, Character.UnicodeScript.KATAKANA, Character.UnicodeScript.HANGUL)
}
