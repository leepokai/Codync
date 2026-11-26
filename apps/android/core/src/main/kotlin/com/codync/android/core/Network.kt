package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.decodeFromJsonElement

/**
 * The computer's networks of the user's computers as phones see them (`networkView`, the `network` event): read-only,
 * since who is in is decided at a computer (CodyncKit `NetworkView`).
 */
@Serializable
data class NetworkView(
    val joined: Boolean = false,
    /** Every network the computer is in, the default first. */
    val networks: List<NetworkInfo> = emptyList(),
    val assistants: Boolean = false,
    /** Cloud assistants can reach this computer at all: it's in a Codync account. */
    val assistantsAvailable: Boolean = false,
    /** This computer's coding agents' sessions with a Network address; hosts before them send none. */
    val sessions: List<NetworkSession> = emptyList(),
) {
    /** Something to show: a network, cloud assistants that can reach this computer's bots, or coding agents' sessions. */
    val shown: Boolean get() = joined || (assistantsAvailable && assistants) || sessions.isNotEmpty()

    companion object {
        fun decode(value: JsonElement): NetworkView = WireJson.decodeFromJsonElement(value)
    }
}

/**
 * A coding agent's session on the computer (`codync-host mcp network`). [kind], [tier] and [state] stay strings: a
 * newer host may send ones this app doesn't know.
 */
@Serializable
data class NetworkSession(
    /** What bots and other agents message it at. */
    val address: String,
    /** The agent's own title for it; display only. */
    val title: String? = null,
    val kind: String = "other",
    /** The ACP registry id of its logo. */
    val registry: String? = null,
    /** How it gets its mail: `pull` (it checks); later `turn` and `push`. */
    val tier: String = "pull",
    /** Whether its agent runs now (`working`, `idle`, `online`, `offline`); null where nothing can tell. */
    val state: String? = null,
    /** Running now, as far as the computer can tell. */
    val runs: Boolean = false,
    /** Folded under the older ones: neither running nor called in the last day. The host lists running ones first, then the most recent. */
    val older: Boolean = false,
    /** Its last call (Unix ms). */
    val seen: Long? = null,
) {
    /** What its agent does, untranslated, where it says; the dot alone shows online or offline. */
    val stateTitle: String? get() = when (state) { "working" -> "Working"; "idle" -> "Idle"; else -> null }

    /** What the dot says to a screen reader, untranslated, where no word shows (online or offline); with no state, there is no dot. */
    val dotTitle: String? get() = when (state) { "online" -> "Online"; "offline" -> "Offline"; else -> null }

    /** How it gets its mail, untranslated; null for a tier this app doesn't know. */
    val tierTitle: String? get() = when (tier) {
        "pull" -> "Checks its mail"
        "turn" -> "Told of mail at its next step"
        "push" -> "Told of mail as it comes"
        else -> null
    }
}

/** One network the computer is in. */
@Serializable
data class NetworkInfo(
    val id: String,
    /** What the user calls it (`home`). */
    val name: String,
    /** This computer's name in it. */
    val computer: String? = null,
    val role: String = "member",
    val computers: List<NetworkComputer> = emptyList(),
    /** Who on its other computers may message this computer's bots, the first match deciding. */
    val rules: List<NetworkRule> = emptyList(),
    /** What a computer no rule matches gets; hosts before 2.49.0 send none and let it in at allow. */
    val defaultAccess: String = "allow",
) {
    /** The rules' footer, untranslated: what decides, and what a computer no rule matches gets. */
    val rulesFooter: String get() = if (defaultAccess == "allow") {
        "The first rule that matches decides. Without rules, every computer in the network may message every bot here. Answers to this computer's own messages always get in."
    } else {
        "The first rule that matches decides. A computer no rule matches gets Auto. Answers to this computer's own messages always get in."
    }

    /**
     * How [rule] reads: "Allow: {from} → {to}", with the bots' names where this network knows them. [tr] translates
     * each template before its names go in.
     */
    fun describe(rule: NetworkRule, tr: (String) -> String): String {
        val from = when {
            rule.fromComputer == ANY && rule.fromAgent == ANY -> tr("any bot on any computer")
            // `lead@*` (only from the command line) reads "lead on any computer": never wider than it is.
            rule.fromComputer == ANY -> tr("{bot} on any computer").replace("{bot}", rule.fromAgent)
            rule.fromAgent == ANY -> tr("any bot on {computer}").replace("{computer}", rule.fromComputer)
            else -> fill(tr("{bot} on {computer}"), "bot" to agentName(rule.fromComputer, rule.fromAgent), "computer" to rule.fromComputer)
        }
        val to = if (rule.toAgent == ANY) tr("any bot here") else computers.firstOrNull { it.me }?.let { me ->
            me.agents.firstOrNull { it.id == rule.toAgent }?.name
        } ?: rule.toAgent
        // An access this app doesn't know reads as deny, never wider than it is.
        val access = when (rule.access) {
            "allow" -> "Allow: {from} → {to}"
            "auto" -> "Auto: {from} → {to}"
            "ask" -> "Ask: {from} → {to}"
            else -> "Deny: {from} → {to}"
        }
        return fill(tr(access), "from" to from, "to" to to)
    }

    private fun agentName(computer: String, agent: String): String =
        computers.firstOrNull { it.name.equals(computer, ignoreCase = true) }?.agents?.firstOrNull { it.id == agent }?.name ?: agent

    private companion object {
        const val ANY = "*"
    }
}

@Serializable
data class NetworkComputer(
    val name: String,
    /** `owner`, `admin` or `member`. */
    val role: String = "member",
    /** The computer this phone is paired with. */
    val me: Boolean = false,
    val online: Boolean = false,
    /** The bots it offers to the network. */
    val agents: List<NetworkAgent> = emptyList(),
) {
    val roleTitle: String get() = when (role) { "owner" -> "Owner"; "admin" -> "Admin"; else -> "Member" }
    val presence: String get() = when { me -> "This computer"; online -> "Online"; else -> "Away" }
}

@Serializable
data class NetworkAgent(val id: String, val name: String = id)

/** `{fromComputer, fromAgent, toAgent, access}`, `*` meaning any. */
@Serializable
data class NetworkRule(val fromComputer: String = "*", val fromAgent: String = "*", val toAgent: String = "*", val access: String = "deny")
