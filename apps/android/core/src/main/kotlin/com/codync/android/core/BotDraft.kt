package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonObject

@Serializable
data class BotDraft(
    val id: String? = null,
    val name: String = "",
    val description: String = "",
    val avatarColor: String = AVATAR_COLORS.random(),
    val avatarShape: String = AVATAR_SHAPES.random(),
    val backend: String = "claude",
    val command: String? = null,
    val cwd: String = "",
    /** `ask` or `auto`: whether Codync asks about what the agent asks, or approves it. */
    val permission: String = "ask",
    val model: String? = null,
    val modelOptions: Map<String, String> = emptyMap(),
    val fastMode: Boolean? = null,
    val pinned: Boolean = false,
    val hidden: Boolean = false,
    val notify: Boolean? = null,
    val connectors: List<String>? = null,
    /** Unset on a new bot, like [connectors]: the host turns on every installed one. */
    val skills: List<String>? = null,
    val computer: Boolean = false,
    val manageTeam: Boolean = false,
    /** Null where the host has none (an older one), so it isn't sent. */
    val fallbacks: List<Fallback>? = null,
    val selfFallbacks: Boolean? = null,
    /** Null where the host has none, or on a new bot, so it isn't sent. */
    val offeredToNetwork: Boolean? = null,
    val reachableByAssistants: Boolean? = null,
    /** The agent mode it works in ([AgentMode]); null, sent as null, keeps the agent's default. */
    val mode: String? = AgentMode.AUTO,
) {
    /** A saved bot keeps a name; only a new one goes without (the host names it after its first conversations). */
    val isValid: Boolean get() = name.isNotBlank()
    fun toggleNotifications(): BotDraft = copy(notify = !(notify ?: true))
    /** On agent [id]: its model starts over, and it leaves the fallbacks (each agent appears once in the chain). */
    fun withAgent(id: String): BotDraft = copy(backend = id, model = null, modelOptions = emptyMap(), fallbacks = fallbacks?.filter { it.backend != id },
        // A picked mode becomes the new agent's Auto mode (or, without one, asking).
        mode = mode?.let { AgentMode.AUTO })
    fun body(): JsonObject {
        val normalized = if (backend == "custom") this else copy(command = null)
        // updateBot merges a patch: null clears model, command or the connector choice, but an unset
        // switch or fallback list must stay out (the host's booleans and lists don't take null).
        return JsonObject(WireJson.encodeToJsonElement(normalized).jsonObject.filterNot { (key, value) -> value == JsonNull && key in UNSET })
    }
    /** A template carries the bot's setup, not its fallbacks (they name this computer's agents): readable JSON, keys sorted as on iOS. */
    fun template(): String = TemplateJson.encodeToString(JsonObject.serializer(),
        JsonObject(body().filterKeys { it !in TEMPLATE_SKIPS }.toSortedMap()))

    /**
     * This draft with each field it hasn't changed from [base] (the bot as the form loaded it) taken from [fresh] (the
     * bot as the host has it now), so saving the form keeps a change made elsewhere: a bot that manages the team,
     * another device.
     */
    fun rebased(base: BotDraft, fresh: BotDraft): BotDraft {
        val (mine, old, new) = listOf(this, base, fresh).map { WireJson.encodeToJsonElement(it).jsonObject }
        return WireJson.decodeFromJsonElement(JsonObject(mine.mapValues { (key, value) -> if (value == old[key]) new.getValue(key) else value }))
    }

    companion object {
        /** A new bot starts with a random face, like iOS. */
        val AVATAR_COLORS = listOf("black", "brown", "red", "orange", "yellow", "green", "cyan", "blue", "violet", "magenta", "gray")
        private val UNSET = setOf("fastMode", "notify", "fallbacks", "selfFallbacks", "offeredToNetwork", "reachableByAssistants")
        /** Who reaches a bot belongs to its computer, not its setup. */
        private val TEMPLATE_SKIPS =
            setOf("id", "pinned", "hidden", "fallbacks", "selfFallbacks", "offeredToNetwork", "reachableByAssistants")
        private val TemplateJson = Json { prettyPrint = true; prettyPrintIndent = "  " }
        val AVATAR_SHAPES = listOf("blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop")

        fun from(bot: Bot): BotDraft = BotDraft(bot.id, bot.name, bot.description, bot.avatarColor, bot.avatarShape,
            bot.backend, bot.command, if (bot.managedWorkspace) "" else bot.cwd, bot.permission, bot.model,
            bot.modelOptions, bot.fastMode, bot.pinned, bot.hidden, bot.notify, bot.connectors, bot.skills, bot.computer, bot.manageTeam ?: false, bot.fallbacks, bot.selfFallbacks,
            bot.offeredToNetwork, bot.reachableByAssistants, bot.mode)
    }
}
