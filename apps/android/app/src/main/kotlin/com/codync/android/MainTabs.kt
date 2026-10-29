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
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
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

private enum class AppTab { Bots, State, Settings }

/**
 * The system tab bar over Bots, State and Settings, as iOS `RootView.tabs`: every tab stays alive, the bar
 * floats as glass over their content and hides while a conversation is open.
 */
@Composable internal fun MainTabs(state: AppState, store: AppStore, account: () -> Unit, manage: () -> Unit, usage: () -> Unit,
    newBot: () -> Unit, newGroup: () -> Unit, edit: (Bot) -> Unit, chat: @Composable () -> Unit) {
    var tab by rememberSaveable { mutableStateOf(AppTab.Bots) }
    // Opening a bot (notification, widget, link) always lands on the Bots tab.
    if (state.selectedBot != null && tab != AppTab.Bots) tab = AppTab.Bots
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(when (tab) {
        AppTab.Bots -> AnalyticsScreen.Bots
        AppTab.State -> AnalyticsScreen.State
        AppTab.Settings -> AnalyticsScreen.Settings
    }))
    val bottom = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    val barSpace = bottom + 62.dp + 24.dp
    GlassStage(Modifier.fillMaxSize(), chrome = {
        // An access request still waiting after its sheet closed: the code to check, until the computer answers.
        val ticket = state.access
        AnimatedVisibility(ticket != null && state.selectedBot == null, Modifier.align(Alignment.BottomCenter).padding(bottom = barSpace - 16.dp),
            enter = fadeIn(Motion.fade()) + slideInVertically(Motion.layoutOffset) { it }, exit = fadeOut(Motion.fade()) + slideOutVertically(Motion.layoutOffset) { it }) {
            if (ticket != null) AccessBanner(ticket, state.cloudComputers.firstOrNull { it.computerId == ticket.computerId }?.name ?: "Your computer",
                store::cancelAccess)
        }
        AnimatedVisibility(state.selectedBot == null, Modifier.align(Alignment.BottomCenter),
            enter = fadeIn(Motion.fade()) + slideInVertically(Motion.layoutOffset) { it / 2 },
            exit = fadeOut(Motion.fade()) + slideOutVertically(Motion.layoutOffset) { it / 2 }) {
            Box(contentAlignment = Alignment.BottomCenter) {
                ScrollEdge(Edge.Bottom, barSpace)
                GlassTabBar(tab, listOf(TabItem(AppTab.Bots, "Bots", "bubble.left.and.bubble.right.fill", 33.dp),
                    TabItem(AppTab.State, "State", "rectangle.stack.fill"), TabItem(AppTab.Settings, "Settings", "gearshape")),
                    { tab = it }, Modifier.padding(bottom = maxOf(bottom - 4.dp, 12.dp)))
            }
        }
    }) {
        Box(Modifier.fillMaxSize().tabLayer(tab == AppTab.Bots)) {
            NavStack(listOfNotNull("roster", state.selectedBot), key = { it }, pop = { store.openBot(null) }) { entry ->
                if (entry == "roster") BotListView(state, store, account, manage, newBot, newGroup, edit, barSpace)
                else chat()
            }
        }
        Box(Modifier.fillMaxSize().tabLayer(tab == AppTab.State)) { StateScreen(state, store, usage, barSpace, active = tab == AppTab.State) }
        Box(Modifier.fillMaxSize().tabLayer(tab == AppTab.Settings)) { SettingsView(state, store, barSpace) }
    }
}

/** One tab's content: visible and interactive only while selected; the other sits under it, deaf to touches. */
private fun Modifier.tabLayer(selected: Boolean): Modifier = zIndex(if (selected) 1f else 0f)
    .graphicsLayer { alpha = if (selected) 1f else 0f }
    .then(if (selected) Modifier else Modifier.clearAndSetSemantics {}.pointerInput(Unit) {
        awaitPointerEventScope { while (true) awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() } }
    })
