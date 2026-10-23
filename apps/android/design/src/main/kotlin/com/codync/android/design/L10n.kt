package com.codync.android.design

import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf

/** Which Chinese writing the interface uses. Traditional is its own table. */
enum class ChineseScript { Hans, Hant }

/** `zh-Hant`, `zh-TW`, `zh-HK` and `zh-MO` read Traditional. Other Chinese locales read Simplified. */
fun systemScript(tag: String): ChineseScript? {
    val id = tag.lowercase().replace('_', '-').substringBefore('.')
    if (!id.startsWith("zh")) return null
    return if (id.contains("hant") || id.contains("-tw") || id.contains("-hk") || id.contains("-mo")) ChineseScript.Hant else ChineseScript.Hans
}

/** The language of this app's own interface. Chats and bots are not translated. `Zh` stays stored as "zh". */
enum class AppLanguage {
    System, Zh, ZhHant, En;

    fun usesChinese(systemIsChinese: Boolean): Boolean = when (this) {
        Zh, ZhHant -> true
        En -> false
        System -> systemIsChinese
    }

    /** The table this choice reads. A missing Traditional line can fall back to Simplified. */
    fun resolvedScript(systemTag: String): ChineseScript? = when (this) {
        Zh -> ChineseScript.Hans
        ZhHant -> ChineseScript.Hant
        En -> null
        System -> systemScript(systemTag)
    }

    companion object {
        fun fromStored(raw: String?): AppLanguage = when (raw) {
            "zh", "zh-Hans" -> Zh
            "zh-Hant" -> ZhHant
            "en" -> En
            else -> System
        }

        fun stored(language: AppLanguage): String = when (language) {
            System -> "system"
            Zh -> "zh"
            ZhHant -> "zh-Hant"
            En -> "en"
        }

        /** Language names stay in that language. "Follow system" is translated with the interface. */
        fun menuTitle(language: AppLanguage, preference: AppLanguage, systemIsChinese: Boolean, phrases: Map<String, String>): String =
            when (language) {
                System -> translate("Follow system", preference, systemIsChinese, phrases)
                Zh -> "简体中文"
                ZhHant -> "繁體中文"
                En -> "English"
            }
    }
}

/** One piece of a phrase key such as "Runs the official installer on {name}." */
private data class Piece(val literal: String?, val token: String?)

/**
 * A sentence the host or a view already filled in, matched back to its `{token}` phrase.
 * A slot that is itself a known phrase (a setup hint) is translated too.
 */
private val templateLock = Any()
private var cachedKey: Map<String, String>? = null
private var cachedTemplates: List<Triple<Int, String, List<Piece>>> = emptyList()

/** Parsed once per phrase-table instance. Callers keep one map for the process. */
private fun templates(phrases: Map<String, String>): List<Triple<Int, String, List<Piece>>> {
    synchronized(templateLock) {
        if (cachedKey === phrases) return cachedTemplates
    }
    val built = phrases.keys.mapNotNull { key ->
        val pieces = parseTemplate(key) ?: return@mapNotNull null
        val literal = pieces.sumOf { it.literal?.length ?: 0 }
        if (literal < 6) null else Triple(literal, key, pieces)
    }.sortedWith(compareByDescending<Triple<Int, String, List<Piece>>> { it.first }.thenByDescending { it.second.length })
    synchronized(templateLock) {
        cachedKey = phrases
        cachedTemplates = built
    }
    return built
}

private fun templated(text: String, phrases: Map<String, String>, depth: Int = 0): String? {
    for ((_, key, pieces) in templates(phrases)) {
        val phrase = phrases[key] ?: continue
        fill(text, pieces, phrase, phrases, depth)?.let { return it }
    }
    return null
}

/** `{name}` and the same shape. A `{` that is not a slot makes the key unusable as a template. */
private fun parseTemplate(template: String): List<Piece>? {
    val pieces = mutableListOf<Piece>()
    var rest = template
    while (rest.contains('{')) {
        val open = rest.indexOf('{')
        if (open > 0) pieces.add(Piece(rest.substring(0, open), null))
        val close = rest.indexOf('}', open)
        if (close < 0) return null
        val token = rest.substring(open, close + 1)
        val name = token.substring(1, token.length - 1)
        if (name.isEmpty() || name.any { !it.isLetterOrDigit() } || pieces.lastOrNull()?.token != null) return null
        pieces.add(Piece(null, token))
        rest = rest.substring(close + 1)
    }
    if (rest.isNotEmpty()) pieces.add(Piece(rest, null))
    return if (pieces.any { it.token != null }) pieces else null
}

private fun fill(text: String, pieces: List<Piece>, phrase: String, phrases: Map<String, String>, depth: Int): String? {
    var pos = 0
    val caps = linkedMapOf<String, String>()
    for ((index, piece) in pieces.withIndex()) {
        val literal = piece.literal
        if (literal != null) {
            if (!text.startsWith(literal, pos)) return null
            pos += literal.length
            continue
        }
        val token = piece.token ?: return null
        val next = pieces.getOrNull(index + 1)?.literal
        val end = if (next == null) text.length else text.indexOf(next, pos)
        if (end < 0 || end == pos) return null
        caps[token] = text.substring(pos, end)
        pos = end
    }
    if (pos != text.length) return null
    var result = phrase
    for ((token, value) in caps) {
        val inner = phrases[value] ?: if (depth == 0) templated(value, phrases, 1) else null
        result = result.replace(token, inner ?: value)
    }
    return result
}

fun translate(text: String, preference: AppLanguage, systemIsChinese: Boolean, phrases: Map<String, String>): String {
    if (!preference.usesChinese(systemIsChinese)) return text
    phrases[text]?.let { return it }
    templated(text, phrases)?.let { return it }
    val trimmed = text.trim()
    if (trimmed == text) return text
    return phrases[trimmed] ?: templated(trimmed, phrases) ?: text
}

/** Defaults keep source strings, so a test that doesn't provide a language stays in English. */
val LocalAppLanguage = staticCompositionLocalOf { AppLanguage.System }
val LocalSystemChinese = staticCompositionLocalOf { false }
val LocalPhrases = staticCompositionLocalOf { emptyMap<String, String>() }
val LocalSetAppLanguage = staticCompositionLocalOf<(AppLanguage) -> Unit> { {} }

@Composable fun tr(text: String): String =
    translate(text, LocalAppLanguage.current, LocalSystemChinese.current, LocalPhrases.current)
