package com.codync.android.core

import kotlinx.serialization.Serializable

/** A login of the chain: an agent on one of its login profiles, or (with none) on the login the computer already has. */
data class Login(val backend: String, val loginProfile: String? = null)

/** One step of a bot's fallback chain: an agent (the bot's own too, on another [loginProfile]), and the model to use on it. */
@Serializable
data class Fallback(val backend: String = "", val command: String? = null, val model: String? = null, val modelOptions: Map<String, String> = emptyMap(),
    val loginProfile: String? = null) {
    val login: Login get() = Login(backend, loginProfile)
}

/** The fallback a bot runs on right now (`agentStep`): its step, agent and model, since when, and why it left the one before. */
@Serializable
data class AgentStep(val step: Int = 0, val agent: String = "", val model: String? = null, val since: Long = 0, val reason: String? = null,
    val loginProfile: String? = null) {
    val login: Login get() = Login(agent, loginProfile)
}

/** The login the bot is set to run on. */
val Bot.login: Login get() = Login(backend, loginProfile)

/** [login]'s rest, unless it's over. */
fun Bot.restOf(login: Login, now: Long = System.currentTimeMillis()): Resting? = resting.firstOrNull { Login(it.agent, it.loginProfile) == login && it.until > now }

/**
 * The Agent card's first line while the bot runs on a fallback; null on its own agent. [name] says what to call a login
 * ("Claude Code (Work)" on a login profile), the computer's own by its name too when asked, which tells two logins of one
 * agent apart; [clock] writes a time.
 */
fun Bot.stepBanner(name: (Login, own: Boolean) -> String, clock: (Long) -> String, now: Long = System.currentTimeMillis()): String? {
    val step = agentStep?.login ?: return null
    val same = step.backend == backend
    val (to, own) = name(step, same) to name(login, same)
    val rest = restOf(login, now)
    return if (rest != null) "Using $to until $own resets at ${clock(rest.until)}" else "Using $to while $own can't answer"
}

/** The login the draft is set to run on. */
val BotDraft.login: Login get() = Login(backend, loginProfile)

/** The logins the chain has, the bot's own first: each is in it once, so an agent comes again only on another of its login profiles. */
val BotDraft.logins: List<Login> get() = listOf(login) + fallbacks.orEmpty().map { it.login }

/** The logins of this agent a new step can still take: the computer's own (first, as null) and its signed-in login profiles, without the ones in [taken]. */
fun AgentBackend.freeLogins(taken: List<Login>): List<String?> =
    (listOf<String?>(null) + loginProfiles.orEmpty().filter { it.signedIn }.map { it.id }).filter { Login(id, it) !in taken }

/** The agent the bot runs on right now: its fallback's while it's on one. */
val Bot.runningAgent: String get() = agentStep?.agent ?: backend

/** The bots run more than one agent, so the roster marks each with its agent's logo. */
fun List<Bot>.mixesAgents(): Boolean = filter { it.kind != "group" }.mapTo(HashSet()) { it.runningAgent }.size > 1

/** A login of a bot's chain that rests until `until` (ms): `usageLimit`, `rateLimited`, `signedOut`… */
@Serializable
data class Resting(val agent: String = "", val until: Long = 0, val reason: String = "", val loginProfile: String? = null) {
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
