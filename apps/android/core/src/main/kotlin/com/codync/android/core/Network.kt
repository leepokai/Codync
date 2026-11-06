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
) {
    /** Something to show: a network, or cloud assistants that can reach this computer's bots. */
    val shown: Boolean get() = joined || (assistantsAvailable && assistants)

    companion object {
        fun decode(value: JsonElement): NetworkView = WireJson.decodeFromJsonElement(value)
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
) {
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
            else -> tr("{bot} on {computer}").replace("{bot}", agentName(rule.fromComputer, rule.fromAgent))
                .replace("{computer}", rule.fromComputer)
        }
        val to = if (rule.toAgent == ANY) tr("any bot here") else computers.firstOrNull { it.me }?.let { me ->
            me.agents.firstOrNull { it.id == rule.toAgent }?.name
        } ?: rule.toAgent
        // An access this app doesn't know reads as deny, never wider than it is.
        val access = if (rule.access == "allow") "Allow: {from} → {to}" else "Deny: {from} → {to}"
        return tr(access).replace("{from}", from).replace("{to}", to)
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
