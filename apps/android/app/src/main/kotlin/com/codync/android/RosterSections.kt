package com.codync.android

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.delay

/** Shared 1-second clock that ticks only while at least one working bot is visible. */
@Composable internal fun rememberRosterClock(active: Boolean): Long {
    var now by remember { mutableStateOf(System.currentTimeMillis()) }
    LaunchedEffect(active) {
        if (!active) return@LaunchedEffect
        while (true) {
            now = System.currentTimeMillis()
            delay(1_000L)
        }
    }
    return now
}

/** Search field, quick filter chips, and workspace grouping toggle for the roster. */
@Composable internal fun RosterControlsHeader(
    query: String,
    onQueryChange: (String) -> Unit,
    filter: RosterFilter,
    onFilterSelect: (RosterFilter) -> Unit,
    groupByProject: Boolean,
    onGroupByProjectChange: (Boolean) -> Unit,
    showGroupByProject: Boolean,
    bots: List<Bot>,
    modifier: Modifier = Modifier,
) {
    val haptics = LocalCodyncHaptics.current
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SearchField(
            text = query,
            onChange = onQueryChange,
            prompt = "Search bots, folders, or messages",
        )
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            for (option in RosterFilter.entries) {
                val count = when (option) {
                    RosterFilter.All -> null
                    else -> bots.count(option::matches).takeIf { it > 0 }
                }
                val label = if (count != null) "${option.label} ($count)" else option.label
                FilterChipPill(
                    label = label,
                    selected = filter == option,
                    accentDot = if (option == RosterFilter.NeedsInput && (count ?: 0) > 0) Palette.warning else null,
                ) {
                    if (filter != option) haptics.perform(CodyncHaptic.TabSwitch)
                    onFilterSelect(option)
                }
            }
            if (showGroupByProject) {
                FilterChipPill(
                    label = "By project",
                    selected = groupByProject,
                    icon = "folder",
                ) {
                    haptics.perform(CodyncHaptic.Toggle)
                    onGroupByProjectChange(!groupByProject)
                }
            }
        }
    }
}

@Composable private fun FilterChipPill(
    label: String,
    selected: Boolean,
    icon: String? = null,
    accentDot: Color? = null,
    onClick: () -> Unit,
) {
    val fill = if (selected) Palette.accentFill else Palette.surface
    val ink = if (selected) Palette.onAccent else Palette.secondary
    Row(
        Modifier.height(32.dp)
            .clip(Capsule())
            .background(fill)
            .clickable(role = Role.Tab, onClick = onClick)
            .semantics {
                contentDescription = label
                this.selected = selected
            }
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        if (accentDot != null && !selected) {
            Box(Modifier.size(6.dp).clip(Capsule()).background(accentDot))
        }
        if (icon != null) {
            Symbol(icon, size = 13.dp, tint = ink)
        }
        Text(label, style = Type.caption.medium, color = ink, maxLines = 1)
    }
}

/**
 * Top attention banner for bots in `needsInput` or with pending permissions.
 * Never approves blindly: only exposes direct Allow/Deny when a single pending `permission` entry
 * with full context and options is loaded; otherwise opens the bot's conversation.
 */
@Composable internal fun AttentionBannerSection(
    items: List<BotAttentionItem>,
    bots: List<Bot>,
    store: AppStore,
    modifier: Modifier = Modifier,
) {
    if (items.isEmpty()) return
    val haptics = LocalCodyncHaptics.current
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        for (item in items) {
            Column(
                Modifier.fillMaxWidth()
                    .clip(RoundedRectangle(18.dp))
                    .background(Palette.surface)
                    .padding(14.dp)
                    .semantics { contentDescription = "Attention required for ${item.bot.name}" },
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Row(
                    Modifier.fillMaxWidth().clickable(role = Role.Button) { store.openBot(item.bot.id) },
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    BotAvatar(item.bot, bots, 28.dp)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text(item.bot.name, style = Type.subheadline.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Box(Modifier.size(7.dp).clip(Capsule()).background(Palette.warning))
                        }
                        when (item) {
                            is BotAttentionItem.DirectPermission -> {
                                Text(item.headline, style = Type.caption.medium, color = Palette.warning, maxLines = 1)
                            }
                            is BotAttentionItem.OpenChatRequired -> {
                                Text(item.summary, style = Type.caption, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
                            }
                        }
                    }
                    Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
                }
                when (item) {
                    is BotAttentionItem.DirectPermission -> {
                        Text(
                            item.title,
                            Modifier.fillMaxWidth().clip(RoundedRectangle(10.dp)).background(Palette.background).padding(horizontal = 10.dp, vertical = 6.dp),
                            style = Type.caption.monospaced,
                            color = Palette.secondary,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                            Box(
                                Modifier.height(34.dp).clip(Capsule()).background(Palette.accentFill)
                                    .clickable(enabled = !item.answering, role = Role.Button) {
                                        haptics.perform(CodyncHaptic.PermissionAction)
                                        store.respondToAttention(item.bot, item.entry.id, item.allowOptionId)
                                    }
                                    .padding(horizontal = 14.dp),
                                contentAlignment = Alignment.Center,
                            ) {
                                if (item.answering) Spinner(13.dp, color = Palette.onAccent)
                                else Text(item.allowLabel, style = Type.caption.semibold, color = Palette.onAccent)
                            }
                            Box(
                                Modifier.height(34.dp).clip(Capsule()).background(Palette.bubbleUser)
                                    .clickable(enabled = !item.answering, role = Role.Button) {
                                        haptics.perform(CodyncHaptic.PermissionAction)
                                        store.respondToAttention(item.bot, item.entry.id, item.denyOptionId)
                                    }
                                    .padding(horizontal = 14.dp),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(item.denyLabel, style = Type.caption.semibold, color = Palette.danger)
                            }
                            Spacer(Modifier.weight(1f))
                            LinkButton("Details", style = Type.caption.medium, color = Palette.secondary) {
                                store.openBot(item.bot.id)
                            }
                        }
                    }
                    is BotAttentionItem.OpenChatRequired -> {
                        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                            SecondaryButton("Review in chat") { store.openBot(item.bot.id) }
                        }
                    }
                }
            }
        }
    }
}

/** Collapsible workspace section header. */
@Composable internal fun WorkspaceSectionHeader(
    section: WorkspaceSection,
    collapsed: Boolean,
    onToggle: () -> Unit,
) {
    val turn by animateFloatAsState(if (collapsed) 0f else 90f, Motion.layout(), label = "SectionChevron")
    val icon = when (section.key.kind) {
        WorkspaceSectionKind.Project -> "folder"
        WorkspaceSectionKind.Managed -> "sparkles"
        WorkspaceSectionKind.Groups -> "person.2"
        WorkspaceSectionKind.Unassigned -> "tray"
    }
    Row(
        Modifier.fillMaxWidth()
            .padding(top = 10.dp, bottom = 4.dp)
            .clip(RoundedRectangle(12.dp))
            .background(Palette.surface)
            .clickable(role = Role.Button, onClick = onToggle)
            .semantics {
                contentDescription = "${section.title}, ${section.bots.size} bots"
                stateDescription = if (collapsed) "Collapsed" else "Expanded"
            }
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Symbol(icon, size = 15.dp, tint = Palette.secondary)
        Column(Modifier.weight(1f)) {
            Text(section.title, style = Type.subheadline.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (!section.subtitle.isNullOrBlank()) {
                Text(section.subtitle, style = Type.caption2, color = Palette.tertiary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Text("${section.bots.size}", style = Type.caption.semibold, color = Palette.secondary)
        Symbol("chevron.right", Modifier.rotate(turn), size = 12.dp, tint = Palette.tertiary)
    }
}

/** Message hits section shown when searching in the roster. */
@Composable internal fun MessageSearchHitsSection(
    searchState: RosterSearchState,
    bots: List<Bot>,
    onOpenHit: (RosterMessageHit) -> Unit,
) {
    if (searchState.query.isBlank() || searchState.messageHits.isEmpty()) return
    Column(
        Modifier.fillMaxWidth().padding(top = 12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(
            Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text("Messages (${searchState.messageHits.size})", style = Type.subheadline.semibold, color = Palette.text)
            if (searchState.isPartialCache) {
                Text("Cached on this phone", style = Type.caption, color = Palette.tertiary)
            }
        }
        for (hit in searchState.messageHits) {
            val bot = bots.firstOrNull { it.id == hit.botId }
            Row(
                Modifier.fillMaxWidth()
                    .clip(RoundedRectangle(14.dp))
                    .background(Palette.surface)
                    .clickable(role = Role.Button) { onOpenHit(hit) }
                    .padding(12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                if (bot != null) BotAvatar(bot, bots, 28.dp)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(hit.botName, style = Type.subheadline.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        if (hit.threadId != null) {
                            MetadataPill("Thread", icon = "arrowshape.turn.up.left")
                        }
                        Spacer(Modifier.weight(1f))
                        if (hit.createdAt > 0) {
                            Text(RelativeTime.day(hit.createdAt), style = Type.caption, color = Palette.tertiary)
                        }
                    }
                    Text(hit.snippet, style = Type.subheadline, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

/** Empty state when active filters or search query match no bots. */
@Composable internal fun EmptyFilteredRoster(
    query: String,
    filter: RosterFilter,
    isPartialCache: Boolean,
    error: String?,
    onClear: () -> Unit,
) {
    Column(
        Modifier.fillMaxWidth().padding(vertical = 32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Symbol(if (error != null) "exclamationmark.triangle" else "magnifyingglass", size = 28.dp, tint = Palette.tertiary)
        Text(
            text = when {
                error != null -> "Search couldn't complete"
                query.isNotBlank() -> "No matching bots or messages"
                else -> "No bots in \"${filter.label}\""
            },
            style = Type.headline,
            color = Palette.text,
            textAlign = TextAlign.Center,
        )
        val detail = when {
            error != null -> error
            query.isNotBlank() && isPartialCache -> "Searched bot details and messages cached on this phone."
            query.isNotBlank() -> "No bots, folders, or messages matched \"$query\"."
            else -> "Try clearing the filter to see all bots."
        }
        Text(detail, style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center)
        SecondaryButton("Clear filters", action = onClear)
    }
}
