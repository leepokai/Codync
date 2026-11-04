package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.Entry
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject

/**
 * Quick filter chips for the bot roster.
 * Applied identically across List and Constellation presentations while preserving host order.
 */
internal enum class RosterFilter(val label: String) {
    All("All"),
    NeedsInput("Needs you"),
    Working("Working"),
    Unread("Unread"),
    Groups("Groups");

    fun matches(bot: Bot): Boolean = when (this) {
        All -> true
        NeedsInput -> bot.status == "needsInput"
        Working -> bot.status == "working"
        Unread -> bot.unread > 0
        Groups -> bot.kind == "group"
    }
}

/**
 * A cached conversation message matching the roster search query.
 * Preserves `threadId` so jumping opens the exact main chat (`threadId == null`) or thread reply.
 */
internal data class RosterMessageHit(
    val entryId: String,
    val botId: String,
    val botName: String,
    val threadId: String?,
    val snippet: String,
    val createdAt: Long,
)

/**
 * Result of a roster search query across local bot metadata and cached conversation history.
 * `requestId` guards against late async completions replacing a newer query.
 */
internal data class RosterSearchState(
    val query: String = "",
    val requestId: Long = 0L,
    val botMatches: List<Bot> = emptyList(),
    val messageHits: List<RosterMessageHit> = emptyList(),
    val isPartialCache: Boolean = false,
    val error: String? = null,
)

/**
 * Filters `bots` (already in host `roster()` order) by `filter` and optional metadata `query`.
 * Never mutates host order or confuses `needsInput` with `working`.
 */
internal fun filterRosterBots(
    bots: List<Bot>,
    filter: RosterFilter,
    query: String = "",
    state: AppState? = null,
): List<Bot> {
    val trimmed = query.trim()
    return bots.filter { bot ->
        filter.matches(bot) && (trimmed.isEmpty() || botMatchesQuery(bot, trimmed, state))
    }
}

internal fun botMatchesQuery(bot: Bot, query: String, state: AppState? = null): Boolean {
    val q = query.trim()
    if (q.isEmpty()) return true
    val backend = if (bot.kind == "group" || bot.backend.isBlank()) ""
    else "${bot.backend} ${backendName(state, bot.backend)}"
    val workspace = when {
        bot.kind == "group" -> ""
        bot.managedWorkspace -> "Personal workspace"
        else -> bot.cwd
    }
    return listOf(
        bot.name,
        bot.description,
        workspace,
        backend,
        bot.lastMessage.orEmpty(),
    ).any { it.contains(q, ignoreCase = true) }
}

/**
 * Searches cached chat entries in the local mirror for visible bots.
 * Labels results as partial (`isPartialCache = true`) when offline or when any visible bot's
 * full history has not been fetched onto this phone yet.
 */
internal fun searchCachedMessages(
    bots: List<Bot>,
    entries: List<Entry>,
    historyComplete: Set<String>,
    online: Boolean,
    query: String,
    requestId: Long = 0L,
    state: AppState? = null,
    limit: Int = 20,
): RosterSearchState {
    val q = query.trim()
    if (q.isEmpty()) return RosterSearchState(query = "", requestId = requestId, botMatches = bots)
    val visibleById = bots.associateBy(Bot::id)
    val matchingBots = bots.filter { botMatchesQuery(it, q, state) }
    val hits = entries
        .asSequence()
        .filter { entry ->
            entry.botId in visibleById &&
                (entry.kind == "user" || entry.kind == "agent") &&
                entry.isChat &&
                entry.text.contains(q, ignoreCase = true)
        }
        .sortedWith(compareByDescending<Entry> { it.createdAt }.thenByDescending { it.seq })
        .take(limit)
        .map { entry ->
            val bot = visibleById.getValue(entry.botId)
            RosterMessageHit(
                entryId = entry.id,
                botId = bot.id,
                botName = bot.name,
                threadId = entry.threadId,
                snippet = extractMatchSnippet(entry.text, q),
                createdAt = entry.createdAt,
            )
        }
        .toList()
    val partial = !online || bots.any { it.id !in historyComplete }
    return RosterSearchState(
        query = q,
        requestId = requestId,
        botMatches = matchingBots,
        messageHits = hits,
        isPartialCache = partial,
        error = null,
    )
}

internal fun extractMatchSnippet(text: String, query: String, radius: Int = 44): String {
    val clean = text.lines().map(String::trim).filter(String::isNotEmpty).joinToString(" ")
        .replace("**", "").replace("`", "")
    val index = clean.indexOf(query, ignoreCase = true)
    if (index < 0 || clean.length <= radius * 2) return clean
    val start = (index - radius).coerceAtLeast(0)
    val end = (index + query.length + radius).coerceAtMost(clean.length)
    val prefix = if (start > 0) "…" else ""
    val suffix = if (end < clean.length) "…" else ""
    return prefix + clean.substring(start, end).trim() + suffix
}

/**
 * Fast mode badge state for a bot row.
 * Distinguishes active Fast mode, agent-refused Fast mode, and off/unsupported.
 */
internal enum class FastBadgeState { None, Active, Refused }

/**
 * Compact row metadata model.
 * Groups never inherit an arbitrary member's `cwd` or backend.
 */
internal data class BotRowMetadata(
    val agentLabel: String?,
    val workspaceLabel: String?,
    val fastBadge: FastBadgeState,
    val groupMembersLabel: String?,
)

internal fun resolveBotRowMetadata(bot: Bot, state: AppState? = null): BotRowMetadata {
    if (bot.kind == "group") {
        val count = bot.members.size
        return BotRowMetadata(
            agentLabel = null,
            workspaceLabel = null,
            fastBadge = FastBadgeState.None,
            groupMembersLabel = if (count > 0) "$count ${if (count == 1) "bot" else "bots"}" else null,
        )
    }
    val agent = bot.backend.takeIf(String::isNotBlank)?.let { id -> backendName(state, id) }
    val workspace = when {
        bot.managedWorkspace -> "Personal"
        bot.cwd.isNotBlank() -> workspaceFolderName(bot.cwd)
        else -> null
    }
    val fast = when {
        !bot.fastMode || bot.fastModeSupport == "unsupported" -> FastBadgeState.None
        bot.fastModeSupport == "refused" -> FastBadgeState.Refused
        else -> FastBadgeState.Active
    }
    return BotRowMetadata(
        agentLabel = agent,
        workspaceLabel = workspace,
        fastBadge = fast,
        groupMembersLabel = null,
    )
}

internal fun workspaceFolderName(rawPath: String): String? {
    val normalized = rawPath.trim().trimEnd('/', '\\')
    if (normalized.isEmpty()) return null
    val name = normalized.substringAfterLast('/').substringAfterLast('\\').trim()
    return name.ifEmpty { normalized }
}

/**
 * Canonical workspace grouping key and section model.
 * Includes `computerId` and the declared `rawPath` (never just basename) so identically named
 * folders on different computers or different parent directories never merge.
 */
internal enum class WorkspaceSectionKind { Project, Managed, Groups, Unassigned }

internal data class WorkspaceKey(
    val computerId: String,
    val kind: WorkspaceSectionKind,
    val rawPath: String,
) {
    val storageKey: String get() = "$computerId:${kind.name}:$rawPath"
}

internal data class WorkspaceSection(
    val key: WorkspaceKey,
    val title: String,
    val subtitle: String?,
    val bots: List<Bot>,
)

internal fun groupBotsByWorkspace(
    bots: List<Bot>,
    computerId: String,
    computerName: String? = null,
): List<WorkspaceSection> {
    val grouped = linkedMapOf<WorkspaceKey, MutableList<Bot>>()
    for (bot in bots) {
        val trimmedPath = bot.cwd.trim()
        val key = when {
            bot.kind == "group" -> WorkspaceKey(computerId, WorkspaceSectionKind.Groups, "")
            bot.managedWorkspace -> WorkspaceKey(computerId, WorkspaceSectionKind.Managed, "")
            trimmedPath.isNotEmpty() -> WorkspaceKey(computerId, WorkspaceSectionKind.Project, trimmedPath)
            else -> WorkspaceKey(computerId, WorkspaceSectionKind.Unassigned, "")
        }
        grouped.getOrPut(key) { mutableListOf() }.add(bot)
    }
    val basenameCounts = grouped.keys
        .filter { it.kind == WorkspaceSectionKind.Project }
        .groupingBy { workspaceFolderName(it.rawPath).orEmpty().lowercase() }
        .eachCount()

    return grouped.map { (key, sectionBots) ->
        when (key.kind) {
            WorkspaceSectionKind.Project -> {
                val folder = workspaceFolderName(key.rawPath) ?: key.rawPath
                val duplicateBasename = (basenameCounts[folder.lowercase()] ?: 0) > 1
                val subtitle = when {
                    duplicateBasename && !computerName.isNullOrBlank() -> "$computerName · ${key.rawPath}"
                    duplicateBasename -> key.rawPath
                    else -> computerName
                }
                WorkspaceSection(key, folder, subtitle, sectionBots)
            }
            WorkspaceSectionKind.Managed -> WorkspaceSection(key, "Personal workspace", computerName, sectionBots)
            WorkspaceSectionKind.Groups -> WorkspaceSection(key, "Group chats", null, sectionBots)
            WorkspaceSectionKind.Unassigned -> WorkspaceSection(key, "Other bots", computerName, sectionBots)
        }
    }
}

/**
 * Formats live elapsed working duration from `bot.startedAt`.
 * Returns `null` when the bot isn't working or `startedAt` is absent/invalid, never inventing a clock.
 */
internal fun formatWorkingDuration(bot: Bot, nowMillis: Long): String? {
    if (bot.status != "working") return null
    val raw = bot.startedAt ?: return null
    if (raw <= 0L) return null
    val startedMillis = if (raw < 10_000_000_000L) raw * 1000L else raw
    val elapsedSeconds = ((nowMillis - startedMillis) / 1000L)
    if (elapsedSeconds < 0L) return null
    val hours = elapsedSeconds / 3600L
    val minutes = (elapsedSeconds % 3600L) / 60L
    val seconds = elapsedSeconds % 60L
    return if (hours > 0L) "%d:%02d:%02d".format(hours, minutes, seconds)
    else "%d:%02d".format(minutes, seconds)
}

/**
 * Attention banner items for bots needing user action.
 * Only enables direct one-tap approval when there is a single pending `permission` entry
 * with complete options and context; otherwise directs the user to open the conversation.
 */
internal sealed interface BotAttentionItem {
    val bot: Bot

    data class DirectPermission(
        override val bot: Bot,
        val entry: Entry,
        val headline: String,
        val title: String,
        val allowOptionId: String,
        val allowLabel: String,
        val denyOptionId: String,
        val denyLabel: String,
        val answering: Boolean,
    ) : BotAttentionItem

    data class OpenChatRequired(
        override val bot: Bot,
        val summary: String,
        val pendingCount: Int,
    ) : BotAttentionItem
}

internal fun resolveAttentionItems(
    bots: List<Bot>,
    entries: List<Entry>,
    busy: Set<String>,
): List<BotAttentionItem> {
    val pendingByBot = entries
        .filter { it.kind == "permission" && it.data.text("status") == "pending" }
        .groupBy(Entry::botId)

    return bots
        .filter { it.status == "needsInput" || !pendingByBot[it.id].isNullOrEmpty() }
        .map { bot ->
            val pending = pendingByBot[bot.id].orEmpty()
            if (pending.size == 1) {
                resolveSinglePermission(bot, pending.first(), busy)
            } else {
                val count = pending.size
                val summary = when {
                    count > 1 -> "$count requests waiting for your review"
                    bot.activity.isNotBlank() -> bot.activity
                    else -> "Open chat to review what ${bot.name} needs"
                }
                BotAttentionItem.OpenChatRequired(bot = bot, summary = summary, pendingCount = count)
            }
        }
}

private fun resolveSinglePermission(bot: Bot, entry: Entry, busy: Set<String>): BotAttentionItem {
    val d = entry.data
    val options = (d["options"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }
    val allowOpt = options.firstOrNull { it.text("kind") == "allow_once" }
        ?: options.firstOrNull { it.text("kind")?.startsWith("allow") == true }
    val denyOpt = options.firstOrNull { it.text("kind") == "reject_once" }
        ?: options.firstOrNull { it.text("kind")?.startsWith("reject") == true }
    val allowId = allowOpt?.text("optionId")?.takeIf(String::isNotBlank)
    val denyId = denyOpt?.text("optionId")?.takeIf(String::isNotBlank)
    val title = d.text("title")?.takeIf(String::isNotBlank)
        ?: d.text("command")?.takeIf(String::isNotBlank)
    if (allowId == null || denyId == null || title == null) {
        return BotAttentionItem.OpenChatRequired(
            bot = bot,
            summary = bot.activity.ifEmpty { "Needs your approval in chat" },
            pendingCount = 1,
        )
    }
    val headline = when (d.text("toolKind")) {
        "execute" -> "Wants to run a command"
        "edit", "delete", "move" -> "Wants to change files"
        "fetch" -> "Wants to access the web"
        "read", "search" -> "Wants to read files"
        "question" -> "Asks you a question"
        "plan" -> "Wants your approval of a plan"
        else -> "Wants to use a tool"
    }
    return BotAttentionItem.DirectPermission(
        bot = bot,
        entry = entry,
        headline = headline,
        title = title,
        allowOptionId = allowId,
        allowLabel = if (allowOpt.text("kind") == "allow_once") "Allow once" else (allowOpt.text("name") ?: "Allow"),
        denyOptionId = denyId,
        denyLabel = if (denyOpt.text("kind") == "reject_once") "Deny" else (denyOpt.text("name") ?: "Deny"),
        answering = "permission:${entry.id}" in busy,
    )
}

/**
 * Long-press peek preview message model.
 * Distinguishes main conversation messages (`threadId == null`) from thread replies (`threadId != null`).
 */
internal data class PeekMessage(
    val id: String,
    val authorLabel: String,
    val text: String,
    val isUser: Boolean,
    val threadId: String?,
    val createdAt: Long,
)

internal fun resolvePeekMessages(bot: Bot, entries: List<Entry>, limit: Int = 3): List<PeekMessage> =
    entries
        .asSequence()
        .filter { it.botId == bot.id && (it.kind == "user" || it.kind == "agent") && it.isChat && it.text.isNotBlank() }
        .sortedBy(Entry::seq)
        .toList()
        .takeLast(limit)
        .map { entry ->
            val isUser = entry.kind == "user"
            val author = if (isUser) "You" else entry.data.text("author")?.takeIf(String::isNotBlank) ?: bot.name
            PeekMessage(
                id = entry.id,
                authorLabel = author,
                text = messagePreview(entry.text),
                isUser = isUser,
                threadId = entry.threadId,
                createdAt = entry.createdAt,
            )
        }
