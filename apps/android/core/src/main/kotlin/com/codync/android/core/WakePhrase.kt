package com.codync.android.core

/** A foreground, on-device transcript gate; never sends audio to a wake-word service. */
class WakePhrase(private val enabled: Boolean, private val clock: () -> Long = { System.nanoTime() / 1_000_000 }) {
    private var awakeUntil = 0L
    fun reset() { awakeUntil = 0 }
    fun accept(text: String): String? {
        val input = text.trim()
        if (!enabled) return input.takeIf(String::isNotEmpty)
        if (awakeUntil > clock()) { reset(); return input.takeIf(String::isNotEmpty) }
        reset()
        val match = prefix.find(input) ?: return null
        val command = input.substring(match.range.last + 1).trim()
        if (command.isEmpty()) { awakeUntil = clock() + 15_000; return null }
        return command
    }
    companion object { private val prefix = Regex("^hey[\\s\\p{P}]+hermes\\b[\\s\\p{P}]*", RegexOption.IGNORE_CASE) }
}
