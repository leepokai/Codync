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
 * A sentence before it's translated: the phrase, and the names to put in once it's read in the app's language.
 */
data class StepBanner(val key: String, val values: Map<String, String>)

/** Puts a translated phrase's names in: a login profile named "Work while traveling" stays one name. */
fun fillTemplate(text: String, values: Map<String, String>): String =
    Regex("\\{(\\w+)\\}").replace(text) { match -> values[match.groupValues[1]] ?: match.value }

/**
 * The Agent card's first line while the bot runs on a fallback, as a phrase and its names; null on its own agent. [name]
 * says what to call a login ("Claude Code (Work)" on a login profile), the computer's own by its name too when asked,
 * which tells two logins of one agent apart; [clock] writes a time. Translating the phrase before the names go in keeps
 * a name that holds " while " from being read as the sentence's own word.
 */
fun Bot.stepBannerPhrase(name: (Login, own: Boolean) -> String, clock: (Long) -> String, now: Long = System.currentTimeMillis()): StepBanner? {
    val step = agentStep?.login ?: return null
    val same = step.backend == backend
    val (to, own) = name(step, same) to name(login, same)
    val rest = restOf(login, now)
    if (rest == null) return StepBanner("Using {to} while {own} can't answer", mapOf("to" to to, "own" to own))
    // A reset time is real only for a usage limit; another rest names what it is (signed out, rate limited…).
    return if (rest.reason == "usageLimit") StepBanner("Using {to} until {own} resets at {time}", mapOf("to" to to, "own" to own, "time" to clock(rest.until)))
    else StepBanner("Using {to} while {own} ${rest.phrase}", mapOf("to" to to, "own" to own))
}

/** The banner as one English line, for callers that show English; views translate the phrase first. */
fun Bot.stepBanner(name: (Login, own: Boolean) -> String, clock: (Long) -> String, now: Long = System.currentTimeMillis()): String? =
    stepBannerPhrase(name, clock, now)?.let { fillTemplate(it.key, it.values) }

/** The login the draft is set to run on. */
val BotDraft.login: Login get() = Login(backend, loginProfile)

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

    /** Why it rests, in the banner's words: "is signed out", "is rate limited"… */
    val phrase: String get() = when (reason) {
        "usageLimit" -> "reached its usage limit"
        "outOfCredits" -> "is out of credits"
        "rateLimited" -> "is rate limited"
        "overloaded" -> "is overloaded"
        "signedOut" -> "is signed out"
        "notStarting" -> "couldn't start"
        "modelMissing" -> "doesn't have this bot's model"
        "modeMissing" -> "doesn't have this bot's mode"
        else -> "can't answer"
    }
}

/** Most fallbacks a bot has (the host refuses more). */
const val MAX_FALLBACKS = 3

/** The list with the fallback at [index] moved one place up ([by] -1) or down (+1); unchanged at either end. */
fun List<Fallback>.moved(index: Int, by: Int): List<Fallback> {
    val to = index + by
    if (index !in indices || to !in indices) return this
    return toMutableList().apply { add(to, removeAt(index)) }
}
