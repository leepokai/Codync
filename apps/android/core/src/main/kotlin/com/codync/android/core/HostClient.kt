package com.codync.android.core

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNames
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

@OptIn(ExperimentalSerializationApi::class)
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
    /** The login profile it runs on ([LoginProfile]); null for the login the computer already has. */
    val loginProfile: String? = null,
    val cwd: String = "",
    val managedWorkspace: Boolean = false,
    val permission: String = "ask",
    /** The agent mode it works in ([AgentMode]); null for the agent's default. */
    val mode: String? = null,
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
    /** Uses the memory all the user's bots share ("About you"); null from an older host. */
    val sharedMemory: Boolean? = null,
    /** Keeps the harness's own memory (Claude Code's…) on beside Codync's; null where it has no switch. */
    val harnessMemory: Boolean? = null,
    /** The harness keeps one memory for all its sessions that Codync can't turn off (Hermes Agent, Gemini CLI): every bot on it shares it. */
    val harnessMemoryShared: Boolean? = null,
    /** When Codync summarizes its chat; null where it doesn't (an agent it can't ask to, a group, an older host). */
    val compaction: Compaction? = null,
    /** The sizes its agent's window allows; null until the agent has told its window. */
    val compactionTokens: CompactionTokens? = null,
    /** Moves whenever memory changes, so an open memory screen fetches it again. */
    val memoryRev: Long? = null,
    val autoName: Boolean = false,
    val createdAt: Long = 0,
    val startedAt: Long? = null,
    val workingChat: String? = null,
    val lastMessage: String? = null,
    val lastAt: Long = 0,
    val status: String = "idle",
    val activity: String = "",
    val backend: String = "",
    val pinned: Boolean = false,
    /** The computer's lead bot: the host makes the first, and the user may pick another (`setLead`) or delete it. Placed in the roster like any bot, never hidden. */
    val lead: Boolean = false,
    /** The roster section the user put it in; null for none (or a host without sections). Kept while pinned or hidden. */
    val section: String? = null,
    /** Where the user dragged it in the roster, synced by the host to every device. */
    val position: Int = 0,
    val hidden: Boolean = false,
    /** The computer's box of threads started without a bot (desktop "No bot"): hidden, never a row in the roster. */
    val plain: Boolean = false,
    val unread: Int = 0,
    /** How many of [unread] are cards waiting on the user: the badge turns amber. 0 from an older host. */
    val waiting: Int = 0,
    val rev: Long = 0,
    val deleted: Boolean = false,
    /** Agents that carry on, in order, when its own can't answer (out of usage, rate limited, signed out…); null from an older host. */
    val fallbacks: List<Fallback>? = null,
    /** The bot may set its own fallbacks, for example when the user asks it to; null from an older host. */
    val selfFallbacks: Boolean? = null,
    /** Advisor mode: the agent its threads run on; null when they run on the bot's own. */
    val threadAgent: Fallback? = null,
    /** In Advisor mode, the threads' fallbacks; null from an older host (no Advisor mode there). */
    val threadFallbacks: List<Fallback>? = null,
    /** The fallback it runs on right now; null on its own agent. */
    val agentStep: AgentStep? = null,
    /** Agents of its chain resting now. */
    val resting: List<Resting> = emptyList(),
    /** The `/` commands of the agent it runs on now. Empty from an older host, and for a group. */
    val agentCommands: List<AgentCommand> = emptyList(),
    /** What it keeps running on the computer. Empty from an older host, and for a group. */
    val devServers: List<DevServer> = emptyList(),
    /** Its box ([WorkTask]): every live or waiting task in dispatch order, then the ones ended in the last day. Empty from a host before the box, and for a group. */
    @Serializable(with = WorkSerializer::class)
    val work: List<WorkTask> = emptyList(),
    /** The most threads it runs at once; null from a host before the box. */
    // Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0.
    @JsonNames("maxAgents")
    @Serializable(with = MaxThreadsSerializer::class)
    val maxThreads: Int? = null,
    /** A thread that starts while another of its threads works in the same repository gets a git worktree of its own; null from an older host. */
    // Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0.
    @JsonNames("taskWorktrees")
    val threadWorktrees: Boolean? = null,
    /** Its folder is in a git repository: where [threadWorktrees] matters. */
    val gitRepo: Boolean = false,
    /** The user's folders it sends work to, the first its default; empty from a host before projects (`hello.projects`). */
    val projects: List<String> = emptyList(),
    /** Whether each of [projects], in the same order, is in a git repository: where [worktreeMode] matters. */
    val projectGitRepos: List<Boolean> = emptyList(),
    /** Per project (a path in [projects]): the folders, relative to it, linked into each new worktree of it; null from a
     *  host before `hello.worktreeFill`. */
    val sharedFolders: Map<String, List<String>>? = null,
    /** Where its threads work ([WorktreeMode], by its wire name); null from a host before projects. */
    val worktreeMode: String? = null,
    /** Git is on the computer; null from an older host, which is counted as having it ([gitState]). */
    val gitInstalled: Boolean? = null,
    /** How it stands outside its chat ([Outside]); null from an older host. */
    @Serializable(with = OutsideSerializer::class)
    val outside: Outside? = null,
    val customIcon: CustomIcon? = null,
) {
    /** Whether its saved project [path] is in a git repository; false for one it hasn't saved yet. */
    fun isGitProject(path: String): Boolean = projects.zip(projectGitRepos).any { (project, git) -> project == path && git }

    /** The messages in the badge the user hasn't read: what `markRead` clears (waiting cards stay counted). */
    val unseen: Int get() = unread - waiting

    /** Its band in the roster: pinned bots, then the rest. Reordering stays inside a band. */
    val band: Int get() = if (pinned) 0 else 1

    /** An agent bot unless its agent said it has no Fast mode, or one that has it on (so it can be turned off). */
    val fastModeShown: Boolean get() = kind != "group" && (fastMode || fastModeSupport != "unsupported")

    /** Why Fast mode isn't in effect, when the agent turned it off itself. */
    val fastModeRefusal: String? get() = if (fastMode && fastModeSupport == "refused")
        "The agent turned it off: ${fastModeIssue ?: "not available"}. Switch it off and on to try again." else null

    /** A group's bots, in member order, from [bots]. */
    fun memberBots(bots: List<Bot>): List<Bot> = members.mapNotNull { id -> bots.firstOrNull { it.id == id } }

    /** Its status outside its chat: the chat's own from a host that names none. */
    val outsideStatus: String get() = outside?.status ?: status
    /** When what it does outside its chat started. */
    val outsideStartedAt: Long? get() = if (outside != null) outside.startedAt else startedAt

    /** Working anywhere: its own chat, a room or a routine. */
    val isWorking: Boolean get() = status in listOf("working", "needsInput")
    /** Working right now in this chat (its own, or a group it's in). */
    fun isWorking(chat: String): Boolean = isWorking && (workingChat ?: id) == chat
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
            urls = hello["urls"]?.jsonArray?.map { it.jsonPrimitive.content }?.let(::directUrls) ?: computer.urls,
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
