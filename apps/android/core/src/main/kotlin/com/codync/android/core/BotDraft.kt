package com.codync.android.core

import kotlinx.serialization.SerialName
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
    /** When Codync summarizes its chat ([Bot.compaction]); null where it doesn't, so it isn't sent. */
    val compaction: Compaction? = null,
    /** The login profile it runs on; null, sent as null, is the login the computer already has. */
    val loginProfile: String? = null,
    /** The most threads it runs at once ([Bot.maxThreads]); null where the host has none, so it isn't sent. */
    val maxThreads: Int? = null,
    /** A worktree per thread ([Bot.threadWorktrees]); null where the host has none, so it isn't sent. */
    val threadWorktrees: Boolean? = null,
    /** Its projects ([Bot.projects]), the default first; null where the host has none, so neither they nor [cwd] are sent. */
    val projects: List<String>? = null,
    /** Where its threads work ([Bot.worktreeMode]); null where the host has none, or on a new bot, so it isn't sent. */
    val worktreeMode: WorktreeMode? = null,
    /** Per project, the folders linked into each new worktree of it ([Bot.sharedFolders]); null where the host has none, so it isn't sent. */
    val sharedFolders: Map<String, List<String>>? = null,
    /** Advisor mode's agent for its threads ([Bot.threadAgent]); null, sent as null, turns it off. */
    val threadAgent: Fallback? = null,
    /** The threads' fallbacks; null where the host has no Advisor mode, so neither this nor [threadAgent] is sent. */
    val threadFallbacks: List<Fallback>? = null,
    /** Pauses it: it answers nothing and spends nothing until it is resumed. Null from a host without the switch, so it
     *  is not sent — an older host would ignore it, which is the switch that must not be shown. */
    val paused: Boolean? = null,
) {
    /** A saved bot keeps a name; only a new one goes without (the host names it after its first conversations). */
    val isValid: Boolean get() = name.isNotBlank()
    fun toggleNotifications(): BotDraft = copy(notify = !(notify ?: true))
    /** On [login] (an agent on one of its logins): a fallback on that login goes (each login is in the chain once), and
     *  another agent's model starts over. */
    fun withAgent(login: Login): BotDraft = when {
        login == this.login -> this
        login.backend == backend -> copy(loginProfile = login.loginProfile, fallbacks = fallbacks?.filter { it.login != login })
        else -> copy(backend = login.backend, loginProfile = login.loginProfile, model = null, modelOptions = emptyMap(),
            fallbacks = fallbacks?.filter { it.login != login },
            // A picked mode becomes the new agent's Auto mode (or, without one, asking).
            mode = mode?.let { AgentMode.AUTO })
    }
    /** Advisor mode on: its threads start on the bot's own agent, model and fallbacks, for the user to change (on a custom
     *  command, which the threads' picker doesn't offer, on the first installed agent of [backends]); off: none. */
    fun withAdvisor(on: Boolean, backends: List<AgentBackend>): BotDraft = when {
        !on -> copy(threadAgent = null, threadFallbacks = emptyList())
        backend != "custom" -> copy(threadAgent = Fallback(backend, model = model, modelOptions = modelOptions, loginProfile = loginProfile),
            threadFallbacks = fallbacks.orEmpty())
        else -> (backends.firstOrNull { it.available } ?: backends.firstOrNull())?.let { copy(threadAgent = it.stepOn(emptyList()), threadFallbacks = emptyList()) }
            ?: this
    }
    /** Its threads on [login], as [withAgent] puts the bot on one: a threads' fallback on that login goes. */
    fun withThreadAgent(login: Login): BotDraft = if (threadAgent?.login == login) this
        else copy(threadAgent = threadAgent?.takeIf { it.backend == login.backend }?.copy(loginProfile = login.loginProfile)
            ?: Fallback(login.backend, loginProfile = login.loginProfile), threadFallbacks = threadFallbacks?.filter { it.login != login })
    /** [path] added as its last project (one it has stays where it is). */
    fun withProject(path: String): BotDraft = if (path in projects.orEmpty()) this else copy(projects = projects.orEmpty() + path)
    fun withoutProject(path: String): BotDraft = copy(projects = projects.orEmpty() - path, sharedFolders = sharedFolders?.minus(path))
    /** [path] as its default project: moved first. */
    fun withDefaultProject(path: String): BotDraft = copy(projects = listOf(path) + (projects.orEmpty() - path))
    /** The folders linked into each new worktree of [project]. */
    fun sharedFolders(project: String): List<String> = sharedFolders?.get(project).orEmpty()
    /** [name] (see [sharedFolderName]) linked into each new worktree of [project]; one it has, or one the host refuses, changes nothing. */
    fun withSharedFolder(project: String, name: String): BotDraft {
        val folder = sharedFolderName(name)?.takeIf { it !in sharedFolders(project) } ?: return this
        return copy(sharedFolders = sharedFolders.orEmpty() + (project to sharedFolders(project) + folder))
    }
    fun withoutSharedFolder(project: String, name: String): BotDraft =
        copy(sharedFolders = (sharedFolders.orEmpty() + (project to sharedFolders(project) - name)).filterValues { it.isNotEmpty() })
    fun body(): JsonObject {
        val normalized = if (backend == "custom") this else copy(command = null)
        // updateBot merges a patch: null clears model, command or the connector choice, but an unset
        // switch or fallback list must stay out (the host's booleans and lists don't take null). Advisor mode's agent goes
        // as null (off) only to a host that has it.
        return JsonObject(WireJson.encodeToJsonElement(normalized).jsonObject.filterNot { (key, value) ->
            value == JsonNull && key in UNSET || key == "threadAgent" && threadFallbacks == null ||
                // A host with projects keeps the bot in its own folder: a project goes in [projects], never [cwd].
                key == "cwd" && projects != null })
    }
    /** A template carries the bot's setup, not its fallbacks or its threads' agent (they name this computer's agents): readable JSON, keys sorted as on iOS. */
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
        private val UNSET = setOf("fastMode", "notify", "fallbacks", "selfFallbacks", "offeredToNetwork", "reachableByAssistants", "compaction", "maxThreads", "threadWorktrees", "threadFallbacks", "projects", "worktreeMode", "sharedFolders", "paused")
        /** Who reaches a bot and the login profile it runs on belong to its computer, not its setup. */
        private val TEMPLATE_SKIPS =
            setOf("id", "pinned", "hidden", "loginProfile", "fallbacks", "selfFallbacks", "offeredToNetwork", "reachableByAssistants",
                "threadAgent", "threadFallbacks")
        private val TemplateJson = Json { prettyPrint = true; prettyPrintIndent = "  " }
        val AVATAR_SHAPES = listOf("blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop")

        fun from(bot: Bot): BotDraft = BotDraft(bot.id, bot.name, bot.description, bot.avatarColor, bot.avatarShape,
            bot.backend, bot.command, if (bot.managedWorkspace) "" else bot.cwd, bot.permission, bot.model,
            bot.modelOptions, bot.fastMode, bot.pinned, bot.hidden, bot.notify, bot.connectors, bot.skills, bot.computer, bot.manageTeam ?: false, bot.fallbacks, bot.selfFallbacks,
            bot.offeredToNetwork, bot.reachableByAssistants, bot.mode, bot.compaction, bot.loginProfile, bot.maxThreads, bot.threadWorktrees,
            // A host with projects says where threads work: what tells its bots from an older host's.
            projects = bot.projects.takeIf { bot.worktreeMode != null }, worktreeMode = WorktreeMode.of(bot.worktreeMode),
            sharedFolders = bot.sharedFolders,
            threadAgent = bot.threadAgent, threadFallbacks = bot.threadFallbacks, paused = bot.paused)
    }
}

/**
 * A shared folder's name as typed, made what the host takes, a plain path inside the project (`node_modules`, `.cache/pip`):
 * trimmed, no `./` or trailing slash. Null for none, an absolute path or one that climbs out (`..`), which the host refuses.
 */
fun sharedFolderName(input: String): String? {
    val text = input.trim()
    val parts = text.split('/', '\\').filter { it.isNotEmpty() && it != "." }
    val absolute = text.startsWith('/') || text.startsWith('\\') || Regex("^[A-Za-z]:").containsMatchIn(text)
    return parts.takeUnless { it.isEmpty() || absolute || ".." in it }?.joinToString("/")
}

/** Where a bot's threads work (`worktreeMode`), with the line settings shows for each. */
@Serializable
enum class WorktreeMode(val title: String, val detail: String) {
    @SerialName("off") Off("Off", "Threads work in the project itself"),
    @SerialName("sideBySide") SideBySide("Side by side", "A thread gets its own worktree when another thread works in the same repository"),
    @SerialName("always") Always("Always", "Every thread works in a worktree of its own; your checkout is never touched");

    companion object {
        /** The mode named [wire] (its `@SerialName`); null for none or one this app doesn't know. */
        fun of(wire: String?): WorktreeMode? = entries.firstOrNull { serializer().descriptor.getElementName(it.ordinal) == wire }
    }
}
