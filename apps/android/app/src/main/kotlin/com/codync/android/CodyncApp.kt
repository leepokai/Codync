package com.codync.android

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import com.codync.android.core.BotDraft
import com.codync.android.core.CloudComputer
import com.codync.android.core.GroupDraft
import com.codync.android.design.*

private enum class Root { Loading, Welcome, Pairing, Tabs }

/** The app's root, as iOS `RootView`: the welcome and pairing until set up, then the tabs, with every sheet on top. */
@Composable fun CodyncApp(state: AppState, store: AppStore) {
    var welcomed by rememberSaveable { mutableStateOf(false) }
    var showAccount by rememberSaveable { mutableStateOf(false) }
    var accountSettings by rememberSaveable { mutableStateOf(false) }
    var showComputers by rememberSaveable { mutableStateOf(false) }
    var showPairing by rememberSaveable { mutableStateOf(false) }
    var showUsage by rememberSaveable { mutableStateOf(false) }
    var showMarketplace by rememberSaveable { mutableStateOf(false) }
    var access by remember { mutableStateOf<CloudComputer?>(null) }
    var editor by remember { mutableStateOf<BotDraft?>(null) }
    var groupEditor by remember { mutableStateOf<GroupDraft?>(null) }
    var handledNavigation by rememberSaveable { mutableIntStateOf(state.navigationRequest) }
    var handledGeneration by rememberSaveable { mutableIntStateOf(state.generation) }
    fun closeAll() {
        showAccount = false; accountSettings = false; showComputers = false; showPairing = false; showMarketplace = false
        access = null; editor = null; groupEditor = null
    }
    // A new account or a fresh start drops whatever was open for the old one.
    LaunchedEffect(state.generation) { if (handledGeneration != state.generation) { closeAll(); showUsage = false; handledGeneration = state.generation } }
    // A notification, widget or link: close everything, then maybe show usage.
    LaunchedEffect(state.navigationRequest) {
        if (handledNavigation != state.navigationRequest) { closeAll(); showUsage = state.navigationUsage; handledNavigation = state.navigationRequest }
    }
    val signedIn = state.account.userId != null
    AnalyticsMilestones(state)
    val onboarding = !state.setupComplete && state.computers.isEmpty() && state.cloudComputers.isEmpty()
    // Signing in to an account that has computers completes setup, as pairing or skipping does.
    LaunchedEffect(state.cloudComputers.isNotEmpty(), state.loading) {
        if (!state.loading && !state.setupComplete && state.cloudComputers.isNotEmpty()) store.skipSetup()
    }
    val root = when {
        state.loading -> Root.Loading
        onboarding && !welcomed && !signedIn -> Root.Welcome
        onboarding -> Root.Pairing
        else -> Root.Tabs
    }
    BackHandler(root == Root.Pairing && !signedIn) { welcomed = false }
    val openMarketplace = { showAccount = false; accountSettings = false; showComputers = false; showMarketplace = true }
    Box(Modifier.fillMaxSize().background(Palette.background)) {
        AnimatedContent(root, transitionSpec = {
            if (initialState == Root.Welcome && targetState == Root.Pairing)
                (slideInHorizontally(Motion.layoutOffset) { it } + fadeIn(Motion.fade())) togetherWith (slideOutHorizontally(Motion.layoutOffset) { -it } + fadeOut(Motion.fade()))
            else if (initialState == Root.Pairing && targetState == Root.Welcome)
                (slideInHorizontally(Motion.layoutOffset) { -it } + fadeIn(Motion.fade())) togetherWith (slideOutHorizontally(Motion.layoutOffset) { it } + fadeOut(Motion.fade()))
            else fadeIn(Motion.fade()) togetherWith fadeOut(Motion.fade())
        }, label = "Root") { shown ->
            when (shown) {
                Root.Loading -> Box(Modifier.fillMaxSize())
                Root.Welcome -> WelcomeView(state, store) { welcomed = true }
                Root.Pairing -> Box(Modifier.fillMaxSize().safeDrawingPadding()) {
                    PairingView(state, store, onSkip = store::skipSetup) {
                        if (signedIn) AccountSwitcherButton(state) { showAccount = true } else BackButton { welcomed = false }
                    }
                }
                Root.Tabs -> MainTabs(state, store, account = { showAccount = true }, manage = { showComputers = true }, usage = { showUsage = true },
                    newBot = { editor = BotDraft() }, newGroup = { groupEditor = GroupDraft() },
                    edit = { bot -> if (bot.kind == "group") groupEditor = GroupDraft.from(bot) else editor = BotDraft.from(bot) }) {
                    ChatScreen(state, store) { bot -> if (bot.kind == "group") groupEditor = GroupDraft.from(bot) else editor = BotDraft.from(bot) }
                }
            }
        }
    }
    // The account sheet holds its own push (Computers & settings), as the iOS sheet's NavigationStack.
    CodyncSheet(showAccount && (state.setupComplete || signedIn) && !state.loading, { showAccount = false; accountSettings = false }) {
        NavStack(listOfNotNull("account", "settings".takeIf { accountSettings }), key = { it }, pop = { accountSettings = false }) { page ->
            if (page == "account") AccountSwitcherView(state, store, { accountSettings = true }) { showPairing = true }
            else SettingsView(state, store, { accountSettings = false }, { showPairing = true }, { access = it }, openMarketplace)
        }
    }
    CodyncSheet(showComputers && !state.loading, { showComputers = false }) {
        SettingsView(state, store, null, { showPairing = true }, { access = it }, openMarketplace)
    }
    CodyncSheet(access, { access = null }) { AccessRequestView(state, store, it) }
    CodyncSheet(showPairing && !state.loading, { showPairing = false }) { PairingView(state, store, inModal = true) }
    CodyncSheet(showUsage && !state.loading, { showUsage = false }) { UsageScreen(state) { store.refreshUsage(it) } }
    CodyncSheet(showMarketplace && root == Root.Tabs, { showMarketplace = false }) { MarketplaceView(state, store) }
    CodyncSheet(groupEditor.takeIf { !state.loading }, { groupEditor = null }) { draft ->
        key(state.contextId, state.generation, draft.id) { GroupEditor(state, draft, store) }
    }
    CodyncSheet(editor.takeIf { !state.loading }, { editor = null }) { draft ->
        key(state.contextId, state.generation, draft.id) { BotEditor(state, draft, store) }
    }
    // The remote screen opens over the app once its sheets have slid away.
    LaunchedEffect(state.screenRequest) { if (state.screenRequest != null) { closeAll(); showUsage = false } }
    ScreenLayer(state, store)
    // One place for errors the screen showing them doesn't handle itself (pairing, access, the editors do).
    val inline = root == Root.Pairing || showPairing || access != null || editor != null || groupEditor != null
    CodyncDialog("Something went wrong", state.error != null && !inline, store::clearError, state.error, cancel = "OK") { emptyList() }
}

/** The remote screen as a full-window layer that fades in and out (iOS `.codyncOverlay`). */
@Composable private fun ScreenLayer(state: AppState, store: AppStore) {
    val visible = remember { MutableTransitionState(false) }
    visible.targetState = state.screenRequest != null
    if (!visible.currentState && !visible.targetState) return
    Overlay {
        AnimatedVisibility(visible, enter = fadeIn(Motion.layout()), exit = fadeOut(Motion.fade())) {
            ScreenView(state, store, state.screenRequest?.watching, store::closeScreen)
        }
    }
}

/** The app-level analytics events (iOS reports the same): setup finished, signed in, signed out. */
@Composable private fun AnalyticsMilestones(state: AppState) {
    var setup by remember { mutableStateOf<Boolean?>(null) }
    var user by remember { mutableStateOf<String?>(null) }
    var known by remember { mutableStateOf(false) }
    LaunchedEffect(state.loading, state.setupComplete) {
        if (state.loading) return@LaunchedEffect
        if (setup == false && state.setupComplete) Analytics.capture(AnalyticsEvent.OnboardingCompleted)
        setup = state.setupComplete
    }
    LaunchedEffect(state.account.ready, state.account.userId) {
        if (!state.account.ready) return@LaunchedEffect
        val now = state.account.userId
        if (known && now != user) Analytics.capture(if (now != null) AnalyticsEvent.SignedIn else AnalyticsEvent.SignedOut)
        user = now; known = true
    }
}
