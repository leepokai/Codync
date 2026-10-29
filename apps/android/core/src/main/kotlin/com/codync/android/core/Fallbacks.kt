package com.codync.android.core

import kotlinx.serialization.Serializable

/** One step of a bot's fallback chain: another agent, and the model to use on it. */
@Serializable
data class Fallback(val backend: String = "", val command: String? = null, val model: String? = null, val modelOptions: Map<String, String> = emptyMap())

/** The fallback a bot runs on right now (`agentStep`): its step, agent and model, since when, and why it left the one before. */
@Serializable
data class AgentStep(val step: Int = 0, val agent: String = "", val model: String? = null, val since: Long = 0, val reason: String? = null)

/** An agent of a bot's chain that rests until `until` (ms): `usageLimit`, `rateLimited`, `signedOut`… */
@Serializable
data class Resting(val agent: String = "", val until: Long = 0, val reason: String = "") {
    /** Its line under the agent, with `until` as [time]: "Usage resets 3:00 PM", "Resting until 3:00 PM". */
    fun line(time: String): String = if (reason == "usageLimit") "Usage resets $time" else "Resting until $time"
}

/** Most fallbacks a bot has (the host refuses more). */
const val MAX_FALLBACKS = 3

/** The list with the fallback at [index] moved one place up ([by] -1) or down (+1); unchanged at either end. */
fun List<Fallback>.moved(index: Int, by: Int): List<Fallback> {
    val to = index + by
    if (index !in indices || to !in indices) return this
    return toMutableList().apply { add(to, removeAt(index)) }
}
