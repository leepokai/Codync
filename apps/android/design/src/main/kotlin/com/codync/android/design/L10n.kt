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
enum class AppLanguage(val stored: String, val title: String) {
    // The menu's order, as on the iPhone: Chinese first, then English and the rest.
    System("system", "Follow system"), Zh("zh", "简体中文"), ZhHant("zh-Hant", "繁體中文"), En("en", "English"),
    Ja("ja", "日本語"), Ko("ko", "한국어"), Fr("fr", "Français"), De("de", "Deutsch"), Es("es", "Español");

    fun resolved(systemTag: String): AppLanguage {
        if (this != System) return this
        val base = systemTag.lowercase().replace('_', '-').substringBefore('-').substringBefore('.')
        if (base == "zh") return if (systemScript(systemTag) == ChineseScript.Hant) ZhHant else Zh
        return entries.firstOrNull { it != System && it.stored == base } ?: En
    }

    fun usesChinese(systemIsChinese: Boolean): Boolean = this == Zh || this == ZhHant || (this == System && systemIsChinese)

    fun resolvedScript(systemTag: String): ChineseScript? = when (resolved(systemTag)) {
        Zh -> ChineseScript.Hans
        ZhHant -> ChineseScript.Hant
        else -> null
    }

    companion object {
        fun fromStored(raw: String?): AppLanguage = if (raw == "zh-Hans") Zh else entries.firstOrNull { it.stored == raw } ?: System
        fun stored(language: AppLanguage): String = language.stored
        fun menuTitle(language: AppLanguage, preference: AppLanguage, systemIsChinese: Boolean, phrases: Map<String, String>): String =
            if (language == System) phrases["Follow system"] ?: "Follow system" else language.title
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
    if (preference == AppLanguage.En || (preference == AppLanguage.System && !systemIsChinese)) return text
    phrases[text]?.let { return it }
    templated(text, phrases)?.let { return it }
    val trimmed = text.trim()
    if (trimmed == text) return text
    return phrases[trimmed] ?: templated(trimmed, phrases) ?: text
}

/** Defaults keep source strings, so a test that doesn't provide a language stays in English. */
val LocalAppLanguage = staticCompositionLocalOf { AppLanguage.System }
val LocalSystemTag = staticCompositionLocalOf { "en" }
val LocalSystemChinese = staticCompositionLocalOf { false }
val LocalPhrases = staticCompositionLocalOf { emptyMap<String, String>() }
val LocalSetAppLanguage = staticCompositionLocalOf<(AppLanguage) -> Unit> { {} }

@Composable fun tr(text: String): String = translator()(text)

/** [tr] as a function, for code that translates outside composition (a template filled in later). */
@Composable fun translator(): (String) -> String {
    val language = LocalAppLanguage.current.resolved(LocalSystemTag.current)
    val (chinese, phrases) = LocalSystemChinese.current to LocalPhrases.current
    return { translate(it, language, chinese, phrases) }
}
