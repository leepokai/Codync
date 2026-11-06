package com.codync.android

import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlin.math.roundToInt
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/** Shared list row wrapper combining swipe-to-reveal actions with long-press context menu and peek preview. */
@Composable internal fun RosterBotListEntry(
    bot: Bot,
    state: AppState,
    store: AppStore,
    reorderActive: Boolean,
    drag: ContextDrag?,
    nowMillis: Long,
    edit: () -> Unit,
    delete: () -> Unit,
    selected: Boolean = false,
) {
    SwipeableBotRow(bot, state, store, reorderActive = reorderActive) {
        BotRowItem(
            bot = bot,
            state = state,
            store = store,
            drag = drag,
            nowMillis = nowMillis,
            edit = edit,
            delete = delete,
            selected = selected,
        )
    }
}

/** `selected` marks the bot open beside the roster in the split view. */
@Composable internal fun BotRowItem(
    bot: Bot,
    state: AppState,
    store: AppStore,
    drag: ContextDrag?,
    nowMillis: Long,
    edit: () -> Unit,
    delete: () -> Unit,
    selected: Boolean = false,
) {
    var confirmNewSession by remember(bot.id) { mutableStateOf(false) }
    Box(
        Modifier.fillMaxWidth()
            .then(if (selected) Modifier.clip(RoundedRectangle(16.dp)).background(Palette.surface).padding(horizontal = 8.dp) else Modifier)
            .contextActions(
            onClick = { store.openBot(bot.id) },
            reorder = drag.takeUnless { bot.lead },
            preview = { BotPeekCard(bot, state, store, nowMillis) },
        ) {
            botRosterActions(bot, store, edit, delete) { confirmNewSession = true }
        }.semantics {
            contentDescription = bot.name
            if (selected) this.selected = true
            customActions = buildList {
                if (!bot.lead) add(CustomAccessibilityAction(if (bot.pinned) "Unpin" else "Pin") { store.toggleBotPin(bot); true })
                if (bot.unread > 0) {
                    add(CustomAccessibilityAction("Mark as read") { store.markBotRead(bot); true })
                }
                if (bot.status == "working" && "stop:${bot.id}" !in state.busy) {
                    add(CustomAccessibilityAction("Stop") { store.stopBot(bot); true })
                }
                if (!bot.lead) add(CustomAccessibilityAction("Hide from list") { store.hideBotFromRoster(bot); true })
                if (drag != null) {
                    for ((offset, title) in listOf(-1 to "Move up", 1 to "Move down")) {
                        add(CustomAccessibilityAction(title) { moved(roster(state.bots), bot.id, offset)?.let(store::reorder) != null })
                    }
                }
            }
        },
    ) {
        BotRow(bot, state, nowMillis = nowMillis, onStop = { store.stopBot(bot) })
    }
    CodyncDialog("Start a new session?", confirmNewSession, { confirmNewSession = false },
        message = "The conversation stays here, but the agent starts with a fresh context.") {
        listOf(DialogAction("New session") { store.command("newSession", body("botId" to bot.id)) })
    }
}

/** One bot: avatar with its status dot, name, metadata badges, and day, then the live activity or last message. */
@Composable internal fun BotRow(
    bot: Bot,
    state: AppState,
    nowMillis: Long = System.currentTimeMillis(),
    onStop: (() -> Unit)? = null,
) {
    val density = LocalUiDensity.current
    val haptics = LocalCodyncHaptics.current
    val verticalPadding = if (density == UiDensity.Compact) 6.dp else 10.dp
    val avatarSize = if (density == UiDensity.Compact) 42.dp else 46.dp
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(vertical = verticalPadding),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        AvatarWithStatus(bot, state.bots, avatarSize)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(if (density == UiDensity.Compact) 2.dp else 4.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    if (bot.lead) Symbol("star.fill", size = 13.dp, tint = Palette.tertiary, contentDescription = "Lead bot")
                    else if (bot.pinned) Symbol("pin.fill", size = 13.dp, tint = Palette.tertiary)
                    Text(
                        bot.name,
                        Modifier.weight(1f, fill = false),
                        style = Type.body.semibold,
                        color = Palette.text,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Spacer(Modifier.width(8.dp))
                if (bot.lastAt > 0) {
                    Text(
                        RelativeTime.day(bot.lastAt),
                        style = Type.subheadline,
                        color = if (bot.unread > 0) Palette.text else Palette.tertiary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            BotMetadataBadgesRow(bot, state)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Box(Modifier.weight(1f)) { Preview(bot, state, nowMillis) }
                if (bot.status == "working" && onStop != null) {
                    val stopping = "stop:${bot.id}" in state.busy
                    Row(
                        Modifier.height(24.dp)
                            .clip(Capsule())
                            .background(Palette.surface)
                            .clickable(enabled = !stopping, role = Role.Button) {
                                haptics.perform(CodyncHaptic.Toggle)
                                onStop()
                            }
                            .semantics { contentDescription = if (stopping) "Stopping ${bot.name}" else "Stop ${bot.name}" }
                            .padding(horizontal = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        if (stopping) {
                            Spinner(10.dp, color = Palette.secondary)
                            Text("Stopping…", style = Type.caption2.medium, color = Palette.secondary)
                        } else {
                            Symbol("stop.fill", size = 10.dp, tint = Palette.danger)
                            Text("Stop", style = Type.caption2.semibold, color = Palette.danger)
                        }
                    }
                }
                if (bot.unread > 0) {
                    Box(
                        Modifier.height(18.dp).widthIn(min = 18.dp).clip(Capsule()).background(Palette.accentFill)
                            .padding(horizontal = 6.dp).semantics { contentDescription = "${bot.unread} unread" },
                        contentAlignment = Alignment.Center,
                    ) {
                        Text("${bot.unread}", style = Type.caption2.bold, color = Palette.onAccent)
                    }
                }
            }
        }
    }
}

/** Live activity while working (with elapsed duration when `startedAt` is known), otherwise the last message. */
@Composable private fun Preview(bot: Bot, state: AppState, nowMillis: Long) {
    when {
        bot.status == "needsInput" -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            ThinkingOrb(ThinkingOrbState.Listening, 16.dp, Palette.warning)
            Text(bot.activity.ifEmpty { "Needs your approval" }, style = Type.subheadline, color = Palette.warning, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        bot.status == "working" -> {
            val stopping = "stop:${bot.id}" in state.busy
            val duration = formatWorkingDuration(bot, nowMillis)
            val baseText = if (stopping) "Stopping…" else bot.activity.ifEmpty { "Working…" }
            val text = if (duration != null && !stopping) "$baseText · $duration" else baseText
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                ThinkingOrb(size = 13.dp, color = Palette.secondary)
                Text(text, style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        bot.status == "error" -> Text(
            messagePreview(bot.lastMessage ?: "Something went wrong"),
            style = Type.subheadline,
            color = Palette.danger,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        else -> Text(
            messagePreview(
                // The agent and folder already show in the badges above.
                bot.lastMessage ?: if (bot.kind == "group") "${bot.members.size} bots" else "No messages yet",
            ),
            style = Type.subheadline,
            color = Palette.secondary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** The first non-empty line, without Markdown markers (CodyncKit `MessagePreview`). */
internal fun messagePreview(text: String): String {
    val first = text.lines().map(String::trim).firstOrNull(String::isNotEmpty).orEmpty()
    return first.dropWhile { it in "#>-* " }.replace("**", "").replace("`", "")
}

/**
 * Swipe-to-reveal action row.
 * Horizontal swipe reveals explicit buttons (Pin/Unpin, Mark read on start; Stop, Hide on end)
 * without auto-triggering destructive/stop actions on release. Yields whenever `reorderActive` is true.
 */
@Composable internal fun SwipeableBotRow(
    bot: Bot,
    state: AppState,
    store: AppStore,
    reorderActive: Boolean,
    content: @Composable () -> Unit,
) {
    if (bot.lead) { content(); return }
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()
    val haptics = LocalCodyncHaptics.current
    val startActionsCount = if (bot.unread > 0) 2 else 1
    val isWorking = bot.status == "working"
    val endActionsCount = if (isWorking) 2 else 1
    val actionWidthPx = with(density) { 68.dp.toPx() }
    val maxStartPx = startActionsCount * actionWidthPx
    val maxEndPx = endActionsCount * actionWidthPx
    val offsetX = remember(bot.id) { Animatable(0f) }

    LaunchedEffect(reorderActive) {
        if (reorderActive && offsetX.value != 0f) offsetX.snapTo(0f)
    }

    LaunchedEffect(maxStartPx, maxEndPx) {
        val clamped = offsetX.value.coerceIn(-maxEndPx, maxStartPx)
        if (clamped != offsetX.value) offsetX.animateTo(clamped, Motion.layout())
    }

    fun closeActions() {
        scope.launch { offsetX.animateTo(0f, Motion.layout()) }
    }

    Box(Modifier.fillMaxWidth().clip(RoundedRectangle(16.dp))) {
        if (offsetX.value > 2f) {
            Row(
                Modifier.align(Alignment.CenterStart).fillMaxHeight(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                SwipeActionPill(
                    title = if (bot.pinned) "Unpin" else "Pin",
                    icon = if (bot.pinned) "pin.slash" else "pin.fill",
                    fill = Palette.surface,
                    ink = Palette.text,
                ) {
                    haptics.perform(CodyncHaptic.Toggle)
                    closeActions()
                    store.toggleBotPin(bot)
                }
                if (bot.unread > 0) {
                    SwipeActionPill(
                        title = "Read",
                        icon = "checkmark.message",
                        fill = Palette.accentFill,
                        ink = Palette.onAccent,
                    ) {
                        haptics.perform(CodyncHaptic.Toggle)
                        closeActions()
                        store.markBotRead(bot)
                    }
                }
            }
        }
        if (offsetX.value < -2f) {
            Row(
                Modifier.align(Alignment.CenterEnd).fillMaxHeight(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                if (isWorking) {
                    val stopping = "stop:${bot.id}" in state.busy
                    SwipeActionPill(
                        title = if (stopping) "Stopping" else "Stop",
                        icon = "stop.fill",
                        fill = Palette.danger,
                        ink = Color.White,
                        enabled = !stopping,
                    ) {
                        haptics.perform(CodyncHaptic.Toggle)
                        closeActions()
                        store.stopBot(bot)
                    }
                }
                SwipeActionPill(
                    title = "Hide",
                    icon = "eye.slash",
                    fill = Palette.surface,
                    ink = Palette.secondary,
                ) {
                    haptics.perform(CodyncHaptic.Toggle)
                    closeActions()
                    store.hideBotFromRoster(bot)
                }
            }
        }
        Box(
            Modifier.fillMaxWidth()
                .offset { IntOffset(offsetX.value.roundToInt(), 0) }
                .background(Palette.background)
                .pointerInput(bot.id, reorderActive, maxStartPx, maxEndPx) {
                    if (reorderActive) return@pointerInput
                    detectHorizontalDragGestures(
                        onHorizontalDrag = { change, dragAmount ->
                            change.consume()
                            val next = (offsetX.value + dragAmount).coerceIn(-maxEndPx, maxStartPx)
                            scope.launch { offsetX.snapTo(next) }
                        },
                        onDragEnd = {
                            val current = offsetX.value
                            val target = when {
                                current > maxStartPx * 0.4f -> maxStartPx
                                current < -maxEndPx * 0.4f -> -maxEndPx
                                else -> 0f
                            }
                            if (target != 0f) haptics.perform(CodyncHaptic.DragDrop)
                            scope.launch { offsetX.animateTo(target, Motion.layout()) }
                        },
                        onDragCancel = { closeActions() },
                    )
                },
        ) {
            content()
        }
    }
}

@Composable private fun SwipeActionPill(
    title: String,
    icon: String,
    fill: Color,
    ink: Color,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    Column(
        Modifier.width(62.dp).height(52.dp).clip(RoundedRectangle(14.dp)).background(fill)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .semantics { contentDescription = title },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Symbol(icon, size = 16.dp, tint = ink)
        Text(title, style = Type.caption2.medium, color = ink, maxLines = 1)
    }
}

/** Compact metadata badges for agent, project folder, and Fast mode on a bot row. */
@Composable internal fun BotMetadataBadgesRow(bot: Bot, state: AppState) {
    val meta = resolveBotRowMetadata(bot, state)
    val hasAny = meta.agentLabel != null || meta.workspaceLabel != null || meta.fastBadge != FastBadgeState.None || meta.groupMembersLabel != null
    if (!hasAny) return
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        meta.groupMembersLabel?.let { label ->
            MetadataPill(label, icon = "person.2")
        }
        meta.agentLabel?.let { label ->
            MetadataPill(label)
        }
        meta.workspaceLabel?.let { folder ->
            MetadataPill(folder, icon = "folder")
        }
        when (meta.fastBadge) {
            FastBadgeState.Active -> MetadataPill("Fast", icon = "bolt.fill", tint = Palette.accent)
            FastBadgeState.Refused -> MetadataPill("Fast off", icon = "bolt.slash", tint = Palette.warning)
            FastBadgeState.None -> Unit
        }
    }
}

@Composable internal fun MetadataPill(label: String, icon: String? = null, tint: Color = Palette.tertiary) {
    Row(
        Modifier.clip(Capsule()).background(Palette.surface).padding(horizontal = 7.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        if (icon != null) Symbol(icon, size = 10.dp, tint = tint)
        Text(label, style = Type.caption2.medium, color = tint, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/**
 * Long-press peek preview card.
 * Shows the lifted bot row plus up to 3 recent messages from the main chat or thread,
 * fetching bounded history if needed without marking the conversation as read.
 */
@Composable internal fun BotPeekCard(bot: Bot, state: AppState, store: AppStore, nowMillis: Long) {
    val messages = remember(bot.id, state.entries) { resolvePeekMessages(bot, state.entries, limit = 3) }
    var loading by remember(bot.id, state.contextId) { mutableStateOf(false) }
    val online = state.actualConnection is LinkState.Ready

    LaunchedEffect(bot.id, state.contextId, state.generation, online) {
        if (messages.isEmpty() && online && bot.id !in state.historyComplete) {
            loading = true
            try {
                store.peekBotHistory(bot.id, limit = 3)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                // Keep fallback state if offline or request fails.
            } finally {
                loading = false
            }
        }
    }

    Column(
        Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        BotRow(bot, state, nowMillis = nowMillis, onStop = null)
        Hairline()
        when {
            messages.isNotEmpty() -> {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    for (msg in messages) {
                        Column(
                            Modifier.fillMaxWidth()
                                .clip(RoundedRectangle(10.dp))
                                .background(if (msg.isUser) Palette.bubbleUser else Palette.surface)
                                .padding(horizontal = 10.dp, vertical = 6.dp),
                            verticalArrangement = Arrangement.spacedBy(2.dp),
                        ) {
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                Text(msg.authorLabel, style = Type.caption.semibold, color = Palette.text, maxLines = 1)
                                if (msg.threadId != null) {
                                    Text("· Thread", style = Type.caption2, color = Palette.tertiary)
                                }
                            }
                            Text(msg.text, style = Type.caption, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
            loading -> {
                Row(
                    Modifier.fillMaxWidth().padding(vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Spinner(13.dp)
                    Text("Loading recent messages…", style = Type.caption, color = Palette.secondary)
                }
            }
            else -> {
                Text(
                    "No cached messages on this phone yet. Tap to open the conversation.",
                    Modifier.fillMaxWidth().padding(vertical = 4.dp),
                    style = Type.caption,
                    color = Palette.tertiary,
                )
            }
        }
    }
}
