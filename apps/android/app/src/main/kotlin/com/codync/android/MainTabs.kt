package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.zIndex
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.design.*

internal enum class AppTab { Bots, State, Settings }

/**
 * The system tab bar over Bots, State and Settings, as iOS `RootView.tabs`: every tab stays alive, the bar
 * floats as glass over their content and hides while a conversation is open.
 */
@Composable internal fun MainTabs(app: AccountContext, states: List<AppState>, accounts: AccountStore, account: () -> Unit, manage: () -> Unit,
    usage: () -> Unit, newBot: (String) -> Unit, newGroup: (String) -> Unit, edit: (String, Bot) -> Unit, extensions: (String) -> Unit,
    tab: AppTab, onTab: (AppTab) -> Unit, covered: Boolean, chat: @Composable (AppState) -> Unit) {
    // The open chat, on whichever computer it lives (one at a time).
    val open = states.firstOrNull { it.selectedBot != null }
    // Opening a bot (notification, widget, link) always lands on the Bots tab.
    if (open != null && tab != AppTab.Bots) onTab(AppTab.Bots)
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(when (tab) {
        AppTab.Bots -> AnalyticsScreen.Bots
        AppTab.State -> AnalyticsScreen.State
        AppTab.Settings -> AnalyticsScreen.Settings
    }))
    val bottom = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    val barSpace = bottom + 62.dp + 24.dp
    GlassStage(Modifier.fillMaxSize(), chrome = {
        // Access requests still waiting after their sheet closed: the code to check, until the computer answers.
        val tickets = app.access.values.sortedBy { it.requestId }
        AnimatedVisibility(tickets.isNotEmpty() && open == null, Modifier.align(Alignment.BottomCenter).padding(bottom = barSpace - 16.dp),
            enter = fadeIn(Motion.fade()) + slideInVertically(Motion.layoutOffset) { it }, exit = fadeOut(Motion.fade()) + slideOutVertically(Motion.layoutOffset) { it }) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                for (ticket in tickets) key(ticket.computerId) {
                    AccessBanner(ticket, app.cloudComputers.firstOrNull { it.computerId == ticket.computerId }?.name ?: "Your computer") {
                        accounts.cancelAccess(ticket.computerId)
                    }
                }
            }
        }
        AnimatedVisibility(open == null, Modifier.align(Alignment.BottomCenter),
            enter = fadeIn(Motion.fade()) + slideInVertically(Motion.layoutOffset) { it / 2 },
            exit = fadeOut(Motion.fade()) + slideOutVertically(Motion.layoutOffset) { it / 2 }) {
            Box(contentAlignment = Alignment.BottomCenter) {
                ScrollEdge(Edge.Bottom, barSpace)
                GlassTabBar(tab, listOf(TabItem(AppTab.Bots, "Bots", "bubble.left.and.bubble.right.fill", 33.dp),
                    TabItem(AppTab.State, "State", "rectangle.stack.fill"), TabItem(AppTab.Settings, "Settings", "gearshape")),
                    onTab, Modifier.padding(bottom = maxOf(bottom - 4.dp, 12.dp)))
            }
        }
    }) {
        Box(Modifier.fillMaxSize().tabLayer(tab == AppTab.Bots)) {
            // A chat's entry names its computer and bot; it keeps its state while sliding away.
            var shown by remember { mutableStateOf<AppState?>(null) }
            if (open != null) shown = open
            NavStack(listOfNotNull("roster", open?.let { "${it.computer.id}/${it.selectedBot}" } ?: REMOVED_CHAT.takeIf { app.removedChat }),
                key = { it }, pop = { accounts.openBot(null) }) { entry ->
                when (entry) {
                    "roster" -> BotListView(app, states, accounts, account, manage, newBot, newGroup, edit, extensions, barSpace)
                    REMOVED_CHAT -> RemovedComputer { accounts.openBot(null) }
                    else -> (states.firstOrNull { "${it.computer.id}/${it.selectedBot}" == entry } ?: shown)?.let { chat(it) }
                }
            }
        }
        Box(Modifier.fillMaxSize().tabLayer(tab == AppTab.State)) {
            StateScreen(app, states.firstOrNull { it.computer.id == app.currentComputerId }?.usage, accounts, usage, barSpace, active = tab == AppTab.State)
        }
        Box(Modifier.fillMaxSize().tabLayer(tab == AppTab.Settings)) { SettingsView(app, states, accounts, barSpace) }
    }
}

private const val REMOVED_CHAT = "removed"

/** In place of a chat whose computer was removed meanwhile (iOS `ChatScreen`'s empty state). */
@Composable private fun RemovedComputer(back: () -> Unit) {
    Column(Modifier.fillMaxSize().background(Palette.background).statusBarsPadding()) {
        ScreenHeader(leading = { BackButton(back) })
        EmptyState("desktopcomputer.trianglebadge.exclamationmark", "This computer was removed", Modifier.fillMaxSize())
    }
}

/** One tab's content: visible and interactive only while selected; the other sits under it, deaf to touches. */
private fun Modifier.tabLayer(selected: Boolean): Modifier = zIndex(if (selected) 1f else 0f)
    .graphicsLayer { alpha = if (selected) 1f else 0f }
    .then(if (selected) Modifier else Modifier.clearAndSetSemantics {}.pointerInput(Unit) {
        awaitPointerEventScope { while (true) awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() } }
    })
