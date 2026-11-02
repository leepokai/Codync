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
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.BotDraft
import com.codync.android.core.CloudComputer
import com.codync.android.core.GroupDraft
import com.codync.android.core.LinkState
import com.codync.android.design.*

private enum class Root { Loading, Welcome, Pairing, Tabs }

/** A new bot or group chat, on the computer it's made on. */
private data class NewTarget(val group: Boolean, val computerId: String)

/** An editor sheet's draft and the computer its bot lives on. */
private data class Editing<T>(val computerId: String, val draft: T)

/**
 * The app's root, as iOS `RootView`: the welcome and pairing until set up, then the tabs, with every sheet
 * on top. Each computer's screens get that computer's store ([computers] holds their states).
 */
@Composable fun CodyncApp(app: AccountContext, computers: Map<String, AppState>, accounts: AccountStore) {
    var welcomed by rememberSaveable { mutableStateOf(false) }
    var showAccount by rememberSaveable { mutableStateOf(false) }
    var showComputers by rememberSaveable { mutableStateOf(false) }
    var showPairing by rememberSaveable { mutableStateOf(false) }
    var showUsage by rememberSaveable { mutableStateOf(false) }
    var marketplace by rememberSaveable { mutableStateOf<String?>(null) }
    var creating by remember { mutableStateOf<NewTarget?>(null) }
    var access by remember { mutableStateOf<CloudComputer?>(null) }
    var editor by remember { mutableStateOf<Editing<BotDraft>?>(null) }
    var groupEditor by remember { mutableStateOf<Editing<GroupDraft>?>(null) }
    var tab by rememberSaveable { mutableStateOf(AppTab.Bots) }
    // The last link shown, and the store that counted it: requests count from 0 again in each generation and each process.
    var handledNavigation by rememberSaveable { mutableStateOf(accounts.activityOwnerId to 0) }
    var handledGeneration by rememberSaveable { mutableIntStateOf(app.generation) }
    fun closeAll() {
        showAccount = false; showComputers = false; showPairing = false; marketplace = null
        creating = null; access = null; editor = null; groupEditor = null
    }
    // A new account or a fresh start drops whatever was open for the old one.
    LaunchedEffect(app.generation) { if (handledGeneration != app.generation) { closeAll(); showUsage = false; handledGeneration = app.generation } }
    // A notification, widget or link: close everything, then pick the tab as the iPhone does (usage on State, the rest over Bots)
    // and maybe show usage, the computers or extensions.
    LaunchedEffect(app.navigationRequest) {
        val handled = handledNavigation.second.takeIf { handledNavigation.first == accounts.activityOwnerId } ?: 0
        if (handled != app.navigationRequest) {
            closeAll()
            tab = if (app.navigationPage == LinkPage.Usage) AppTab.State else AppTab.Bots
            showUsage = app.navigationPage == LinkPage.Usage
            showComputers = app.navigationPage == LinkPage.Computers
            if (app.navigationPage == LinkPage.Extensions) marketplace = app.currentComputerId
            handledNavigation = accounts.activityOwnerId to app.navigationRequest
        }
    }
    val signedIn = app.account.userId != null
    // Signed in, setup counts as complete (`loadContext`): the account's computers are asked for access from Computers.
    val onboarding = !app.setupComplete && app.computers.isEmpty() && app.cloudComputers.isEmpty()
    val root = when {
        app.loading -> Root.Loading
        onboarding && !welcomed && !signedIn -> Root.Welcome
        onboarding -> Root.Pairing
        else -> Root.Tabs
    }
    BackHandler(root == Root.Pairing && !signedIn) { welcomed = false }
    val states = app.computers.mapNotNull { computers[it.id] }
    val edit = { computerId: String, bot: Bot ->
        if (bot.kind == "group") groupEditor = Editing(computerId, GroupDraft.from(bot)) else editor = Editing(computerId, BotDraft.from(bot))
    }
    val openMarketplace = { id: String -> showAccount = false; showComputers = false; marketplace = id }
    // The sheets too: a bot's editor sets custom icons, which need sign-in.
    CompositionLocalProvider(LocalAccounts provides accounts) {
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
                    Root.Welcome -> WelcomeView(app, accounts) { welcomed = true }
                    Root.Pairing -> Box(Modifier.fillMaxSize().safeDrawingPadding()) {
                        PairingView(app, accounts, onSkip = accounts::skipSetup) {
                            if (signedIn) AccountSwitcherButton(app) { showAccount = true } else BackButton { welcomed = false }
                        }
                    }
                    Root.Tabs -> MainTabs(app, states, accounts, account = { showAccount = true }, manage = { showComputers = true }, usage = { showUsage = true },
                        newBot = { creating = NewTarget(group = false, computerId = it) }, newGroup = { creating = NewTarget(group = true, computerId = it) },
                        edit = edit, extensions = openMarketplace, tab = tab, onTab = { tab = it }, covered = showAccount || showComputers || showPairing || showUsage ||
                            marketplace != null || creating != null || access != null || editor != null || groupEditor != null || app.error != null) { chat ->
                        // A conversation, talking to the computer the bot lives on.
                        accounts.store(chat.computer.id)?.let { store -> ChatScreen(chat, store) { bot -> edit(chat.computer.id, bot) } }
                    }
                }
            }
        }
        CodyncSheet(showAccount && (app.setupComplete || signedIn) && !app.loading, { showAccount = false }) {
            AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Account))
            AccountSwitcherView(app, accounts) { showPairing = true }
        }
        CodyncSheet(showComputers && !app.loading, { showComputers = false }) {
            AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Computers))
            ComputersView(app, states, accounts, { showPairing = true }, { access = it }, openMarketplace)
        }
        CodyncSheet(access, { access = null }) { AccessRequestView(app, accounts, it) }
        CodyncSheet(showPairing && !app.loading, { showPairing = false }) {
            AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Pairing))
            PairingView(app, accounts, inModal = true)
        }
        // Text another app shared: which bot's box it goes to.
        CodyncSheet(app.sharing?.takeIf { root == Root.Tabs }, accounts::finishSharing) { text -> ShareToBot(text, states, accounts) }
        CodyncSheet(showUsage && !app.loading, { showUsage = false }) {
            AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Usage))
            UsageSheet(app, states, accounts)
        }
        // Each computer has its own Extensions; it's opened from that computer's heading or row.
        CodyncSheet(marketplace?.takeIf { root == Root.Tabs && it in computers }, { marketplace = null }) { id ->
            AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Marketplace))
            OnComputer(id, computers, accounts) { state, store -> key(id) { MarketplaceView(state, store) } }
        }
        CodyncSheet(creating?.takeIf { !app.loading }, { creating = null }) { target ->
            OnComputer(target.computerId, computers, accounts) { state, store ->
                // Not keyed on the computer: a new bot keeps its name and face when it moves to another.
                key(app.contextId, app.generation) {
                    if (target.group) NewGroupView(state, store)
                    // A new bot can move to another computer until it's created (iOS `NewBotView`'s Computer row).
                    else NewBotView(state, store, availableComputers(app, states).map { it.computer.id to computerChoice(it) }) {
                        creating = NewTarget(group = false, computerId = it)
                    }
                }
            }
        }
        // A group's editor is its details (a chat's title and the roster open it); a bot's (the roster's Edit profile) is all its settings.
        CodyncSheet(groupEditor.takeIf { !app.loading }, { groupEditor = null }) { target ->
            AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.BotDetails))
            OnComputer(target.computerId, computers, accounts) { state, store ->
                key(app.contextId, app.generation, target.computerId, target.draft.id) { GroupEditor(state, target.draft, store) }
            }
        }
        CodyncSheet(editor.takeIf { !app.loading }, { editor = null }) { target ->
            AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.BotDetails))
            OnComputer(target.computerId, computers, accounts) { state, store ->
                key(app.contextId, app.generation, target.computerId, target.draft.id) { BotEditor(state, target.draft, store) }
            }
        }
        // The remote screen opens over the app once its sheets have slid away: any computer's request (chat header, Computers, `codync://screen`).
        val screen = states.firstOrNull { it.screenRequest != null }
        LaunchedEffect(screen?.computer?.id, screen?.screenRequest) { if (screen != null) { closeAll(); showUsage = false } }
        ScreenLayer(screen, screen?.let { accounts.store(it.computer.id) })
        // One place for every computer's errors the screen showing them doesn't handle itself (pairing, access, the new and edit sheets do).
        val inline = root == Root.Pairing || showPairing || access != null || creating != null || editor != null || groupEditor != null
        val error = app.error ?: states.firstNotNullOfOrNull { it.error }
        CodyncDialog("Something went wrong", error != null && !inline, accounts::clearError, error, cancel = "OK") { emptyList() }
    }
}

/** The account's store, for the few computer screens that also act on the account (custom icons need sign-in). */
internal val LocalAccounts = staticCompositionLocalOf<AccountStore?> { null }

/** [content] with computer [id]'s state and store, while both are there (a sheet of one computer). */
@Composable private fun OnComputer(id: String, computers: Map<String, AppState>, accounts: AccountStore, content: @Composable (AppState, AppStore) -> Unit) {
    val state = computers[id] ?: return
    val store = accounts.store(id) ?: return
    content(state, store)
}

/** Computers that can take a new bot once they're connected: not ones waiting for an update, or that won't come back (iOS `availableStores`). */
internal fun availableComputers(app: AccountContext, states: List<AppState>): List<AppState> =
    states.filter { it.mismatch == null && !app.isStale(it, states) }

/** A computer in the new bot's Computer menu, offline ones marked. */
private fun computerChoice(state: AppState): String =
    if (state.actualConnection is LinkState.Ready) state.hostName else "${state.hostName} (offline)"

/** Usage for one computer at a time: the last active one, which the Usage widget shows too (iOS `UsageSheet`). */
@Composable private fun UsageSheet(app: AccountContext, states: List<AppState>, accounts: AccountStore) {
    val state = states.firstOrNull { it.computer.id == app.currentComputerId }
    val store = accounts.store(state?.computer?.id)
    val picker: @Composable () -> Unit = {
        if (state != null && states.size > 1) DropdownMenu({
            states.map { other -> MenuItem(other.computer.name, selected = other.computer.id == state.computer.id, verbatim = true) { accounts.noteActive(other.computer.id) } }
        }, label = "Computer: ${state.computer.name}") { ComputerBadge(state.hello?.text("device"), 28.dp, state.computer.color) }
    }
    if (state == null || store == null) UsageScreen(null, {}, {}, picker)
    else key(state.computer.id) { UsageScreen(state, store::refreshUsage, store::reloadUsage, picker) }
}

/** The remote screen as a full-window layer that fades in and out (iOS `.codyncOverlay`); it keeps its computer while fading. */
@Composable private fun ScreenLayer(state: AppState?, store: AppStore?) {
    val visible = remember { MutableTransitionState(false) }
    visible.targetState = state != null && store != null
    var shown by remember { mutableStateOf<Pair<AppState, AppStore>?>(null) }
    if (state != null && store != null) shown = state to store
    if (!visible.currentState && !visible.targetState) return
    val (current, owner) = shown ?: return
    Overlay {
        AnimatedVisibility(visible, enter = fadeIn(Motion.layout()), exit = fadeOut(Motion.fade())) {
            AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.RemoteScreen))
            key(current.computer.id) { ScreenView(current, owner, current.screenRequest?.watching, owner::closeScreen) }
        }
    }
}
