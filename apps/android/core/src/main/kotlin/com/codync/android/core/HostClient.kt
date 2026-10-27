package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

@Serializable
data class Bot(
    val id: String,
    val name: String = "",
    val kind: String = "agent",
    val members: List<String> = emptyList(),
    val description: String = "",
    val avatarColor: String = "blue",
    val avatarShape: String = "blob",
    val command: String? = null,
    val cwd: String = "",
    val managedWorkspace: Boolean = false,
    val permission: String = "ask",
    val model: String? = null,
    /** The model's own settings (effort, fast…): agent option id → value. */
    val modelOptions: Map<String, String> = emptyMap(),
    /** The agent's Fast mode: faster replies for more usage. */
    val fastMode: Boolean = false,
    /** `unknown` (no session yet), `offered`, `unsupported` or `refused` (`fastModeIssue` says why). */
    val fastModeSupport: String = "unknown",
    val fastModeIssue: String? = null,
    val notify: Boolean? = null,
    val connectors: List<String> = emptyList(),
    val skills: List<String> = emptyList(),
    val computer: Boolean = false,
    /** The bot can create bots (which can too) and change any bot's setup (the built-in `manage` tools); null from an older host. */
    val manageTeam: Boolean? = null,
    /** The user's other computers list this bot and may message it; null from an older host. */
    val offeredToNetwork: Boolean? = null,
    /** The user's cloud assistant lists this bot and may message it; null from an older host. */
    val reachableByAssistants: Boolean? = null,
    /** The bot that created this one; null when the user did. */
    val createdBy: String? = null,
    val autoName: Boolean = false,
    val createdAt: Long = 0,
    val startedAt: Long? = null,
    val workingChat: String? = null,
    /** The agent's last reported use of this bot's main session; null until reported. */
    val contextUsage: ContextUsage? = null,
    val lastMessage: String? = null,
    val lastAt: Long = 0,
    val status: String = "idle",
    val activity: String = "",
    val backend: String = "",
    val pinned: Boolean = false,
    /** The computer's lead bot, which the host makes: first in the roster, never deleted, hidden or pinned. */
    val lead: Boolean = false,
    /** Where the user dragged it in the roster, synced by the host to every device. */
    val position: Int = 0,
    val hidden: Boolean = false,
    val unread: Int = 0,
    val rev: Long = 0,
    val deleted: Boolean = false,
    /** Agents that carry on, in order, when its own can't answer (out of usage, rate limited, signed out…); null from an older host. */
    val fallbacks: List<Fallback>? = null,
    /** The bot may set its own fallbacks, for example when the user asks it to; null from an older host. */
    val selfFallbacks: Boolean? = null,
    /** The fallback it runs on right now; null on its own agent. */
    val agentStep: AgentStep? = null,
    /** Agents of its chain resting now. */
    val resting: List<Resting> = emptyList(),
) {
    /** Its band in the roster: the lead, then pinned bots, then the rest. Reordering stays inside a band. */
    val band: Int get() = when { lead -> 0; pinned -> 1; else -> 2 }

    /** [agent]'s rest, unless it's over. */
    fun restOf(agent: String, now: Long = System.currentTimeMillis()): Resting? = resting.firstOrNull { it.agent == agent && it.until > now }

    /** An agent bot unless its agent said it has no Fast mode, or one that has it on (so it can be turned off). */
    val fastModeShown: Boolean get() = kind != "group" && (fastMode || fastModeSupport != "unsupported")

    /** Why Fast mode isn't in effect, when the agent turned it off itself. */
    val fastModeRefusal: String? get() = if (fastMode && fastModeSupport == "refused")
        "The agent turned it off: ${fastModeIssue ?: "not available"}. Switch it off and on to try again." else null

    /** A group's bots, in member order, from [bots]. */
    fun memberBots(bots: List<Bot>): List<Bot> = members.mapNotNull { id -> bots.firstOrNull { it.id == id } }

    /** Working right now in this chat (its own, or a group it's in). */
    fun isWorking(chat: String): Boolean = status in listOf("working", "needsInput") && (workingChat ?: id) == chat
}

object HostClient {
    private val json = Json { ignoreUnknownKeys = true }

    suspend fun hello(channel: EncryptedChannel, computer: Computer): Computer {
        val response = channel.call("hello")
        val hello = response.jsonObject
        require(hello.text("computerId") == computer.id && hello.text("signKey") == computer.signKey) {
            "This computer's identity changed. Pair it again."
        }
        val updated = computer.copy(
            name = hello.text("name")?.takeIf(String::isNotBlank) ?: computer.name,
            boxKey = hello.text("boxKey") ?: computer.boxKey,
            urls = hello["urls"]?.jsonArray?.map { it.jsonPrimitive.content }?.filter(::isDirectUrl) ?: computer.urls,
            cloud = hello.text("cloud"),
        )
        updated.validate()
        return updated
    }

    suspend fun bots(channel: EncryptedChannel): List<Bot> {
        // The host exposes its roster through revision-ordered events, like iOS.
        // This finite read never advances the caller's durable mirror cursor.
        return deadline(20_000, "The computer didn't send its bots in time.") { botSnapshot(channel.events(0)) }
    }

    fun decodeBot(event: JsonObject): Bot? {
        if (event.text("type") != "bot") return null
        return decode(event.getValue("bot").jsonObject)
    }

    fun decode(value: JsonObject): Bot {
        val bot = json.decodeFromJsonElement<Bot>(value)
        require(bot.id.isNotBlank() && bot.rev >= 0 && (bot.deleted || bot.name.isNotBlank())) { "The computer sent an invalid bot update." }
        return bot
    }
}
