package com.codync.android.core

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.buildClassSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonEncoder
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.longOrNull

/**
 * When Codync summarizes a bot's chat so it can go on (the host's `compaction`): early by default, near the limit of
 * the agent's window, never (the agent still does when the chat fills up), or at a size the user picked.
 */
@Serializable(with = CompactionSerializer::class)
sealed interface Compaction {
    data object Early : Compaction
    data object NearLimit : Compaction
    data object Off : Compaction
    data class Tokens(val tokens: Long) : Compaction

    /** As its menu row and the pill show it: a phrase to translate, or a size such as "120K" shown as it is. */
    val label: String get() = when (this) {
        Early -> "Early (default)"
        NearLimit -> "Near the limit"
        Off -> "Only when full"
        is Tokens -> sizeLabel(tokens)
    }

    companion object {
        fun sizeLabel(tokens: Long): String = "${tokens / 1000}K"

        /** A size the user typed, in thousands of tokens ("350", "350K", "1.5k"), as tokens; null when it isn't one. */
        fun typed(text: String): Long? {
            val k = text.trim().removeSuffix("k").removeSuffix("K").trim().toDoubleOrNull() ?: return null
            // Past a billion thousands no window holds it, and the tokens would overflow when fitted.
            return if (k > 0 && k < 1e9) Math.round(k * 1000) else null
        }
    }
}

/** The sizes the bot's window allows (`compactionTokens`): [min] to [max] by [step], and the few a menu lists. */
@Serializable
data class CompactionTokens(val min: Long, val max: Long, val step: Long, val presets: List<Long> = emptyList()) {
    /** A typed size as one the window takes: to the nearest [step], within [min] and [max]. */
    fun fit(tokens: Long): Long {
        val step = step.coerceAtLeast(1)
        return ((tokens + step / 2) / step * step).coerceAtLeast(min).coerceAtMost(max)
    }
}

/** `"early"`, `"nearLimit"`, `"off"` or a number of tokens; a name from a newer host reads as early, rather than losing the bot. */
internal object CompactionSerializer : KSerializer<Compaction> {
    override val descriptor = buildClassSerialDescriptor("Compaction")
    override fun serialize(encoder: Encoder, value: Compaction) {
        val wire = when (value) {
            Compaction.Early -> JsonPrimitive("early")
            Compaction.NearLimit -> JsonPrimitive("nearLimit")
            Compaction.Off -> JsonPrimitive("off")
            is Compaction.Tokens -> JsonPrimitive(value.tokens)
        }
        (encoder as JsonEncoder).encodeJsonElement(wire)
    }
    override fun deserialize(decoder: Decoder): Compaction {
        val wire = (decoder as JsonDecoder).decodeJsonElement() as? JsonPrimitive ?: return Compaction.Early
        if (!wire.isString) wire.longOrNull?.let { return Compaction.Tokens(it) }
        return when (wire.content) {
            "nearLimit" -> Compaction.NearLimit
            "off" -> Compaction.Off
            else -> Compaction.Early
        }
    }
}
