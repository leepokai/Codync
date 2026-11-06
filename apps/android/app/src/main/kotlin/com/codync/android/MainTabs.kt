package com.codync.android

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalViewConfiguration
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlin.math.abs

/** The roster's pushed page for a removed computer; bot IDs never take this form. */
private const val REMOVED = "computer/removed"

/**
 * The system tab bar over Bots, State, Extensions and Settings (`SettingsView`), expanding to a side
 * navigation rail and Master-Detail split view on tablets, foldables and desktop windows. Computer
 * management (`ComputersView`) opens over it, from the roster and from Settings.
 */
@Composable internal fun MainTabs(
    state: AppState,
    store: AppStore,
    account: () -> Unit,
    manage: () -> Unit,
    usage: () -> Unit,
    newBot: () -> Unit,
    newGroup: () -> Unit,
    edit: (Bot) -> Unit,
    requestedTab: AppTab? = null,
    requestedTabToken: Int = 0,
    chat: @Composable () -> Unit,
) {
    var tab by rememberSaveable { mutableStateOf(AppTab.Bots) }
    // A chat, or the empty state of a removed computer's chat, covers the roster and hides the tab bar.
    val pushed = state.selectedBot ?: REMOVED.takeIf { state.removedComputer }
    var handledPushed by rememberSaveable { mutableStateOf(pushed) }
    if (pushed != handledPushed) {
        handledPushed = pushed
        if (pushed != null && tab != AppTab.Bots) tab = AppTab.Bots
    }
    var handledTabToken by rememberSaveable { mutableIntStateOf(requestedTabToken) }
    if (requestedTabToken != handledTabToken) {
        handledTabToken = requestedTabToken
        if (requestedTab != null) tab = requestedTab
    }
    // What the destination shows, for the usage analytics; every destination gets its own screen.
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(when (tab) {
        AppTab.Bots -> AnalyticsScreen.Bots
        AppTab.State -> AnalyticsScreen.State
        AppTab.Extensions -> AnalyticsScreen.Marketplace
        AppTab.Computers -> AnalyticsScreen.Settings
    }))

    var reselectBots by rememberSaveable { mutableIntStateOf(0) }
    var reselectState by rememberSaveable { mutableIntStateOf(0) }
    var reselectExtensions by rememberSaveable { mutableIntStateOf(0) }
    var reselectComputers by rememberSaveable { mutableIntStateOf(0) }
    var rosterDragging by remember { mutableStateOf(false) }

    val barStyle by NavigationPreferences.style.collectAsState()
    val autoHidePref by NavigationPreferences.autoHide.collectAsState()
    val touchExploration = rememberTouchExplorationActive()
    val hasCriticalState = state.access != null ||
        state.mismatch != null ||
        state.voice != null ||
        state.bots.any { !it.deleted && it.status == "needsInput" }
    val barVisibility = remember { BarScrollVisibility() }
    barVisibility.canAutoHide = autoHidePref && !touchExploration && !hasCriticalState

    LaunchedEffect(tab, pushed) {
        barVisibility.show()
    }

    val badges = remember(
        state.bots,
        state.connection,
        state.actualConnection,
        state.computers,
        state.cloudComputers,
        state.access,
        state.isStale,
    ) {
        navigationBadges(state)
    }

    val tabItems = remember(badges) {
        AppTab.entries.map { entry ->
            // The fourth destination is this phone's Settings; Computer management opens from the roster and links.
            if (entry == AppTab.Computers) TabItem(entry, "Settings", "gearshape", entry.iconSize, badges.forTab(entry))
            else TabItem(entry, entry.title, entry.icon, entry.iconSize, badges.forTab(entry))
        }
    }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        val expanded = isExpandedWindow(maxWidth, maxHeight)
        val bottom = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
        val barHeight = when (barStyle) {
            NavBarStyle.FloatingCapsule -> 62.dp + 24.dp
            NavBarStyle.CompactDock -> 52.dp + 20.dp
            NavBarStyle.DockedSurface -> 60.dp + 12.dp
        }
        val showVoiceMini = state.voice != null && (state.selectedBot != state.voice.botId || tab != AppTab.Bots)
        val showScreenMini = state.actualConnection is LinkState.Ready && state.screenRequest == null && state.screen?.agentBot != null
        val hasMiniBar = showVoiceMini || showScreenMini
        val barSpace = if (expanded) maxOf(bottom, 20.dp) + if (hasMiniBar) 56.dp else 0.dp
        else bottom + barHeight + if (hasMiniBar) 56.dp else 0.dp
        val showChromeBar = !expanded && pushed == null && barVisibility.visible

        fun selectTab(next: AppTab) {
            barVisibility.show()
            if (next == tab) {
                when (next) {
                    AppTab.Bots -> if (pushed != null && !expanded) store.openBot(null) else reselectBots++
                    AppTab.State -> reselectState++
                    AppTab.Extensions -> reselectExtensions++
                    AppTab.Computers -> reselectComputers++
                }
            } else {
                if (!expanded && pushed != null) store.openBot(null)
                tab = next
            }
        }

        BackHandler(enabled = tab != AppTab.Bots && (expanded || pushed == null)) {
            barVisibility.show()
            tab = AppTab.Bots
        }

        Row(Modifier.fillMaxSize().background(Palette.background)) {
            if (expanded) {
                CodyncNavigationRail(
                    selection = tab,
                    tabs = tabItems,
                    select = ::selectTab,
                )
            }
            GlassStage(
                Modifier
                    .weight(1f)
                    .fillMaxSize()
                    .nestedScroll(barVisibility.nestedScroll)
                    .trackVerticalTouch(barVisibility)
                    .rootTabSwipe(
                        enabled = !expanded && pushed == null && !rosterDragging && !touchExploration,
                        current = tab,
                        select = ::selectTab,
                    ),
                chrome = {
                    TabChrome(
                        state = state,
                        store = store,
                        bannerVisible = state.access != null && (expanded || pushed == null),
                        bannerBottom = (if (showChromeBar) bottom + barHeight else maxOf(bottom, 16.dp)) + if (hasMiniBar) 56.dp else 0.dp,
                        miniBarBottom = if (showChromeBar) bottom + barHeight - 8.dp else maxOf(bottom, 12.dp),
                        showVoiceMini = showVoiceMini,
                        showBar = showChromeBar,
                        barVisibility = barVisibility,
                        bar = {
                            Box(contentAlignment = Alignment.BottomCenter) {
                                ScrollEdge(Edge.Bottom, bottom + barHeight)
                                CodyncBottomBar(
                                    selection = tab,
                                    tabs = tabItems,
                                    select = ::selectTab,
                                    style = barStyle,
                                    bottomInset = bottom,
                                )
                            }
                        },
                        openVoiceBot = { selectTab(AppTab.Bots) },
                    )
                },
            ) {
                DestinationLayers(
                    expanded = expanded,
                    state = state,
                    store = store,
                    tab = tab,
                    account = account,
                    manage = manage,
                    usage = usage,
                    newBot = newBot,
                    newGroup = newGroup,
                    edit = edit,
                    barSpace = barSpace,
                    reselectBots = reselectBots,
                    reselectState = reselectState,
                    reselectExtensions = reselectExtensions,
                    reselectComputers = reselectComputers,
                    onDragActive = { rosterDragging = it },
                    chat = chat,
                )
            }
        }
    }
}

/**
 * What floats over the destinations: the access banner, the call/screen mini bar and the tab bar.
 * Its own `BoxScope` function, so `AnimatedVisibility` isn't taken for `MainTabs`' `RowScope` one.
 */
@Composable private fun BoxScope.TabChrome(
    state: AppState,
    store: AppStore,
    bannerVisible: Boolean,
    bannerBottom: Dp,
    miniBarBottom: Dp,
    showVoiceMini: Boolean,
    showBar: Boolean,
    barVisibility: BarScrollVisibility,
    bar: @Composable () -> Unit,
    openVoiceBot: () -> Unit,
) {
    val ticket = state.access
    AnimatedVisibility(
        bannerVisible,
        Modifier.align(Alignment.BottomCenter).padding(bottom = (bannerBottom - 8.dp).coerceAtLeast(12.dp)),
        enter = fadeIn(Motion.fade()) + slideInVertically(Motion.layoutOffset) { it },
        exit = fadeOut(Motion.fade()) + slideOutVertically(Motion.layoutOffset) { it },
    ) {
        if (ticket != null) {
            AccessBanner(
                ticket,
                state.cloudComputers.firstOrNull { it.computerId == ticket.computerId }?.name ?: "Your computer",
                store::cancelAccess,
            )
        }
    }
    SessionMiniBar(
        state = state,
        store = store,
        showVoice = showVoiceMini,
        onOpenVoiceBot = openVoiceBot,
        modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = miniBarBottom.coerceAtLeast(12.dp)),
    )
    AnimatedVisibility(
        showBar,
        Modifier.align(Alignment.BottomCenter).onFocusChanged { barVisibility.barFocused = it.hasFocus },
        enter = fadeIn(Motion.fade()) + slideInVertically(Motion.layoutOffset) { it / 2 },
        exit = fadeOut(Motion.fade()) + slideOutVertically(Motion.layoutOffset) { it / 2 },
    ) { bar() }
}

@Composable private fun DestinationLayers(
    expanded: Boolean,
    state: AppState,
    store: AppStore,
    tab: AppTab,
    account: () -> Unit,
    manage: () -> Unit,
    usage: () -> Unit,
    newBot: () -> Unit,
    newGroup: () -> Unit,
    edit: (Bot) -> Unit,
    barSpace: Dp,
    reselectBots: Int,
    reselectState: Int,
    reselectExtensions: Int,
    reselectComputers: Int,
    onDragActive: (Boolean) -> Unit,
    chat: @Composable () -> Unit,
) {
    Box(Modifier.fillMaxSize().tabLayer(tab == AppTab.Bots)) {
        if (expanded) {
            BackHandler(enabled = tab == AppTab.Bots && (state.selectedBot != null || state.removedComputer)) {
                store.openBot(null)
            }
            Row(Modifier.fillMaxSize()) {
                Box(Modifier.width(340.dp).fillMaxHeight()) {
                    BotListView(
                        state = state,
                        store = store,
                        account = account,
                        manage = manage,
                        newBot = newBot,
                        newGroup = newGroup,
                        edit = edit,
                        bottomInset = barSpace,
                        reselectToken = reselectBots,
                        highlightSelection = true,
                        onDragActive = onDragActive,
                    )
                }
                Box(Modifier.width(1.dp).fillMaxHeight().background(Palette.border))
                Box(Modifier.weight(1f).fillMaxSize()) {
                    val selected = state.selectedBot
                    if (selected != null) {
                        key(selected) { chat() }
                    } else if (state.removedComputer) {
                        RemovedComputerView({ store.openBot(null) }, manage)
                    } else {
                        EmptyChatDetail(state, newBot)
                    }
                }
            }
        } else {
            NavStack(listOfNotNull("roster", state.selectedBot ?: REMOVED.takeIf { state.removedComputer }), key = { it }, pop = { store.openBot(null) }) { entry ->
                if (entry == "roster") {
                    BotListView(
                        state = state,
                        store = store,
                        account = account,
                        manage = manage,
                        newBot = newBot,
                        newGroup = newGroup,
                        edit = edit,
                        bottomInset = barSpace,
                        reselectToken = reselectBots,
                        onDragActive = onDragActive,
                    )
                } else if (entry == REMOVED) {
                    RemovedComputerView({ store.openBot(null) }, manage)
                } else {
                    chat()
                }
            }
        }
    }
    Box(Modifier.fillMaxSize().tabLayer(tab == AppTab.State)) {
        StateScreen(
            state = state,
            store = store,
            usage = usage,
            bottomInset = barSpace,
            active = tab == AppTab.State,
            reselectToken = reselectState,
        )
    }
    Box(Modifier.fillMaxSize().tabLayer(tab == AppTab.Extensions)) {
        MarketplaceView(
            state = state,
            store = store,
            inModal = false,
            bottomInset = barSpace,
            active = tab == AppTab.Extensions,
            reselectToken = reselectExtensions,
            // Extensions need a computer: Computer management is where one is paired and its extensions opened.
            openComputers = manage,
        )
    }
    Box(Modifier.fillMaxSize().tabLayer(tab == AppTab.Computers)) {
        SettingsView(
            state = state,
            store = store,
            bottomInset = barSpace,
            computers = manage,
            active = tab == AppTab.Computers,
            reselectToken = reselectComputers,
        )
    }
}

@Composable private fun EmptyChatDetail(state: AppState, newBot: () -> Unit) {
    Box(Modifier.fillMaxSize().background(Palette.background).padding(32.dp), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Box(
                Modifier.size(56.dp).clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent),
                contentAlignment = Alignment.Center,
            ) {
                Symbol("bubble.left.and.bubble.right.fill", size = 24.dp, tint = Palette.secondary)
            }
            Text(
                if (state.bots.any { !it.deleted && !it.hidden }) "Select a conversation" else "No bots yet",
                style = Type.title3.semibold,
                color = Palette.text,
                textAlign = TextAlign.Center,
            )
            Text(
                if (state.bots.any { !it.deleted && !it.hidden }) "Choose a bot or group on the left to read and send messages."
                else "Create a bot on ${state.hostName} to start a conversation.",
                style = Type.subheadline,
                color = Palette.secondary,
                textAlign = TextAlign.Center,
            )
            if (state.bots.none { !it.deleted && !it.hidden } && state.computers.isNotEmpty()) {
                PrimaryButton("New bot", action = newBot)
            }
        }
    }
}

/** Tracks whether a real pointer is touching the screen so programmatic `performScrollTo()` in tests never hides the bar. */
private fun Modifier.trackVerticalTouch(visibility: BarScrollVisibility): Modifier = pointerInput(visibility) {
    awaitEachGesture {
        awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
        visibility.touching = true
        try {
            do {
                val event = awaitPointerEvent(PointerEventPass.Final)
            } while (event.changes.any { it.pressed })
        } finally {
            visibility.touching = false
        }
    }
}

/**
 * Horizontal swipe between root tabs when no child view (such as a `LazyRow` or slider) consumed
 * the horizontal gesture and the swipe starts away from the system back-gesture edges.
 */
@Composable private fun Modifier.rootTabSwipe(
    enabled: Boolean,
    current: AppTab,
    select: (AppTab) -> Unit,
): Modifier {
    if (!enabled) return this
    val slop = LocalViewConfiguration.current.touchSlop
    val density = LocalDensity.current
    val edgeGuardPx = with(density) { 28.dp.toPx() }
    val triggerPx = with(density) { 96.dp.toPx() }
    return pointerInput(current) {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Main)
            val width = size.width.toFloat()
            if (down.position.x < edgeGuardPx || down.position.x > width - edgeGuardPx) return@awaitEachGesture
            var totalDx = 0f
            var totalDy = 0f
            var rejected = false
            while (true) {
                val event = awaitPointerEvent(PointerEventPass.Main)
                val change = event.changes.firstOrNull { it.id == down.id } ?: break
                if (!change.pressed) break
                if (change.isConsumed) {
                    rejected = true
                    break
                }
                val dx = change.position.x - change.previousPosition.x
                val dy = change.position.y - change.previousPosition.y
                totalDx += dx
                totalDy += dy
                if (!rejected && (abs(totalDx) > slop || abs(totalDy) > slop)) {
                    if (abs(totalDy) * 1.25f >= abs(totalDx)) {
                        rejected = true
                        break
                    }
                }
            }
            if (!rejected && abs(totalDx) >= triggerPx && abs(totalDx) > abs(totalDy) * 1.6f) {
                val tabs = AppTab.entries
                val index = tabs.indexOf(current)
                val targetIndex = if (totalDx < 0f) index + 1 else index - 1
                if (targetIndex in tabs.indices) {
                    select(tabs[targetIndex])
                }
            }
        }
    }
}

/** One tab's content: visible and interactive only while selected; the other sits under it, deaf to touches. */
private fun Modifier.tabLayer(selected: Boolean): Modifier = zIndex(if (selected) 1f else 0f)
    .graphicsLayer { alpha = if (selected) 1f else 0f }
    .then(if (selected) Modifier else Modifier.clearAndSetSemantics {}.pointerInput(Unit) {
        awaitPointerEventScope { while (true) awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() } }
    })
