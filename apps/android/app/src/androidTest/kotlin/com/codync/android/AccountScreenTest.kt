package com.codync.android

import android.graphics.Bitmap
import android.os.Build
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import com.codync.android.design.LocalDismissModal
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import java.io.File
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext

/** The account sheet and Computers & settings. Read-only: no test confirms a sign-out, route, removal or reset. */
class AccountScreenTest {
    @get:Rule val compose = createComposeRule()
    private val first = SavedAccount("first", "first-session", "alex@example.test", null)
    private val second = SavedAccount("second", "second-session", "sam@example.test", null)
    private val account = AccountState(ready = true, configured = true, userId = first.userId,
        accounts = listOf(first, second), multiple = true)
    private val computer = Computer("paired", "Fixture computer", "fixture-key")
    private fun fixture(accountState: AccountState = account) = AppState(loading = false,
        setupComplete = true, account = accountState, contextId = "profile-fixture")

    @Before fun isolatedEmulatorOnly() { assumeTrue(Build.PRODUCT.startsWith("sdk")) }

    @Test fun signedInOverviewKeepsTheProfileAboveItsNavigation() {
        var scanned = false; var settings = false; var closed = false
        compose.setContent { Hosted { CompositionLocalProvider(LocalDismissModal provides { closed = true }) {
            AccountSwitcherView(fixture(), viewModel(), { settings = true }) { scanned = true }
        } } }
        compose.onNodeWithText("Account").assertIsDisplayed()
        val scan = compose.onNodeWithText("Scan a computer's code").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        val identity = compose.onNodeWithText(first.email).assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        val computers = compose.onNodeWithText("Computers & settings").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        assertTrue(scan.bottom < identity.top)
        assertTrue(identity.bottom < computers.top)
        compose.onNodeWithContentDescription(first.email).assertIsSelected().assertIsNotEnabled()
        compose.onNodeWithContentDescription(second.email).assertIsNotSelected().assertIsEnabled()
        compose.onNodeWithContentDescription("Add account").assertIsDisplayed()
        compose.onNodeWithText("Sign out").assertIsDisplayed()
        compose.onNodeWithText("Continue with Google").assertDoesNotExist()
        compose.onNodeWithText("Start over").assertDoesNotExist()
        snapshot("signed-in")
        compose.onNodeWithText("Scan a computer's code").performClick()
        compose.onNodeWithText("Computers & settings").performClick()
        compose.onNodeWithContentDescription("Close").performClick()
        assertTrue(scanned && settings && closed)
    }

    @Test fun theAddButtonRevealsProvidersOnlyOnDemandAndASessionChangeLocksTheSheet() {
        val state = mutableStateOf(fixture())
        compose.setContent { Hosted { AccountSwitcherView(state.value, viewModel(), {}, {}) } }
        compose.onNodeWithContentDescription("Add account").performClick()
        compose.onNodeWithText("Continue with Apple").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Continue with Google").performScrollTo().assertIsDisplayed()
        compose.onNodeWithContentDescription("Add account").performScrollTo().performClick()
        compose.onNodeWithText("Continue with Google").assertDoesNotExist()
        // The sign-in spinner intentionally keeps drawing; freeze it while checking disabled actions.
        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { state.value = state.value.copy(busy = setOf("session")) }
        compose.mainClock.advanceTimeBy(300)
        compose.onNodeWithContentDescription("Signing in").assertIsDisplayed()
        compose.onNodeWithContentDescription(second.email).assertIsNotEnabled()
        compose.onNodeWithContentDescription("Add account").assertIsNotEnabled()
        compose.onNodeWithText("Sign out").assertIsNotEnabled()
    }

    @Test fun signedOutProvidersAndUnconfiguredPairingRemainAvailable() {
        val state = mutableStateOf(fixture(AccountState(ready = true, configured = true)))
        compose.setContent { Hosted { AccountSwitcherView(state.value, viewModel(), {}, {}) } }
        val apple = compose.onNodeWithText("Continue with Apple").assertIsDisplayed().assertIsEnabled().fetchSemanticsNode().boundsInRoot
        val google = compose.onNodeWithText("Continue with Google").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        assertTrue(apple.bottom <= google.top)
        assertEquals(apple.left, google.left, 1f)
        assertEquals(apple.right, google.right, 1f)
        compose.onNodeWithText("Other sign-in options").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Sign out").assertDoesNotExist()
        compose.runOnUiThread { state.value = state.value.copy(account = state.value.account.copy(ready = false, error = "Couldn't restore sign-in.")) }
        compose.onNodeWithText("Continue with Apple").assertIsNotEnabled()
        compose.onNodeWithText("Continue with Google").assertIsNotEnabled()
        compose.onNodeWithText("Couldn't restore sign-in.").performScrollTo().assertIsDisplayed()
        compose.runOnUiThread { state.value = state.value.copy(account = AccountState(ready = true, configured = false)) }
        compose.onNodeWithText("QR pairing only in this build").assertIsDisplayed()
        compose.onNodeWithText("Continue with Apple").assertDoesNotExist()
        compose.onNodeWithText("Scan a computer's code").assertIsDisplayed()
        compose.onNodeWithText("Computers & settings").assertIsDisplayed()
        snapshot("pairing-only")
    }

    @Test fun signOutAsksFirstAndCancelKeepsTheAccount() {
        compose.setContent { Hosted { AccountSwitcherView(fixture(), viewModel(), {}, {}) } }
        compose.onNodeWithText("Sign out").performClick()
        compose.onNodeWithText("Sign out of ${first.email}?").assertIsDisplayed()
        compose.onNodeWithText("This phone forgets the account's computers. Your bots stay on them.").assertIsDisplayed()
        compose.onNodeWithText("Cancel").performClick()
        compose.onNodeWithText("Sign out of ${first.email}?").assertDoesNotExist()
        compose.onNodeWithText(first.email).assertIsDisplayed()
    }

    @Test fun computerSettingsKeepRoutesNotificationsAndResetBehindConfirmations() {
        val state = fixture().copy(computers = listOf(computer), connection = LinkState.Ready(HostRoute.Direct),
            bots = listOf(Bot("hidden", "Hidden scout", hidden = true)))
        var paired = 0; var popped = 0; var extensions = 0
        compose.setContent { Hosted { SettingsView(state, viewModel(), { popped++ }, { paired++ }, {}) { extensions++ } } }
        compose.onNodeWithText("Computers & settings").assertIsDisplayed()
        compose.onNode(hasText(computer.name) and hasClickAction()).assertIsDisplayed()
        compose.onNodeWithText("1 bot · Wi-Fi/Tailscale").assertIsDisplayed()
        snapshot("computer-card")
        compose.onNodeWithContentDescription("Extensions").performClick()
        assertEquals(1, extensions)
        compose.onNodeWithText("Pair a computer").performClick()
        assertEquals(1, paired)
        compose.onNodeWithText("Hidden scout").performScrollTo().assertIsDisplayed()
        compose.onNodeWithContentDescription("Unhide").assertIsDisplayed()
        if (BuildConfig.PUSH_CONFIGURED) compose.onNode(hasText("Push notifications") and isToggleable()).performScrollTo().assertIsDisplayed()
        else compose.onNodeWithText("Push isn't set up in this build. Bots and chats still update while Codync is open.").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("App version").performScrollTo().assertIsDisplayed()
        snapshot("computers")
        compose.onNodeWithText("Start over").performScrollTo().performClick()
        compose.onNodeWithText("Start over?").assertIsDisplayed()
        compose.onNodeWithText("Cancel").performClick()
        compose.onNodeWithText("Start over?").assertDoesNotExist()
        compose.onNode(hasText(computer.name) and hasClickAction()).performScrollTo().performTouchInput { longClick() }
        compose.onNodeWithText("Remove").performClick()
        compose.onNodeWithText("Remove ${computer.name}?").assertIsDisplayed()
        compose.onNodeWithText("Cancel").performClick()
        compose.onNodeWithContentDescription("Connection").performClick()
        compose.onNodeWithText("Wi-Fi or Tailscale only, never Cloudflare").assertIsDisplayed()
        compose.onNodeWithText("Auto: Wi-Fi or Tailscale, then Cloudflare").assertIsSelected()
    }

    @Test fun accountComputersAskForAccessAndAPendingRequestShowsItsCode() {
        val cloud = CloudComputer("account-computer", "Account computer", "account-key", online = true)
        val asked = mutableListOf<CloudComputer>()
        val state = mutableStateOf(fixture().copy(cloudComputers = listOf(cloud)))
        compose.setContent { Hosted { SettingsView(state.value, viewModel(), null, {}, { asked.add(it) }, {}) } }
        compose.onNodeWithText("Online · In your account").assertIsDisplayed()
        compose.onNodeWithText("Ask for access").performClick()
        assertEquals(listOf(cloud), asked)
        compose.runOnUiThread { state.value = state.value.copy(access = AccessTicket("request", Long.MAX_VALUE, "123456", cloud.computerId, cloud.signKey)) }
        compose.onNodeWithText("Waiting for your OK on the computer · 123456").assertIsDisplayed()
        compose.onNodeWithText("Code").performClick()
        assertEquals(listOf(cloud, cloud), asked)
    }

    @Test fun whileOneComputerIsAskedForAccessTheOthersWait() {
        val asked = CloudComputer("asked-computer", "Asked computer", "asked-key", online = true)
        val other = CloudComputer("other-computer", "Other computer", "other-key", online = true)
        val state = fixture().copy(cloudComputers = listOf(asked, other), askingAccess = asked.computerId)
        compose.setContent { Hosted { SettingsView(state, viewModel(), null, {}, {}, {}) } }
        val buttons = compose.onAllNodesWithText("Ask for access")
        buttons[0].assertIsEnabled()
        buttons[1].assertIsNotEnabled()
    }

    @Test fun thePendingAccessBannerShowsTheCodeAndCancels() {
        var cancelled = false
        compose.setContent { Hosted { AccessBanner(AccessTicket("request", Long.MAX_VALUE, "123456", "computer", "key"), "Fixture computer") { cancelled = true } } }
        compose.onNodeWithText("Approve on Fixture computer").assertIsDisplayed()
        compose.onNodeWithText("Code 123 456").assertIsDisplayed()
        compose.onNodeWithContentDescription("Cancel request").performClick()
        assertTrue(cancelled)
    }

    @Test fun anUnreachableAccountComputerOffersAccessInsteadOfRemoval() {
        val cloud = CloudComputer(computer.id, computer.name, computer.signKey, online = true, access = "granted")
        val state = fixture().copy(computers = listOf(computer), cloudComputers = listOf(cloud),
            connection = LinkState.Failed("This phone cannot reach the computer.", 403))
        var asked: CloudComputer? = null
        compose.setContent { Hosted { SettingsView(state, viewModel(), null, {}, { asked = it }, {}) } }
        compose.onNodeWithText("This phone cannot reach the computer.").assertIsDisplayed()
        compose.onNodeWithText("Ask for access").performClick()
        assertEquals(cloud, asked)
        compose.onNodeWithContentDescription("Remove").assertDoesNotExist()
        compose.onNodeWithText(computer.name).performTouchInput { longClick() }
        compose.onNodeWithText("Remove").assertDoesNotExist()
        compose.onNodeWithText("Revoke this phone's access").assertIsDisplayed()
    }

    @Test fun aRevokedSavedComputerDoesNotBlockAskingItsReplacement() {
        val old = CloudComputer(computer.id, computer.name, computer.signKey, online = false)
        val replacement = CloudComputer("replacement", "New computer", "new-key", online = true)
        val state = fixture().copy(computers = listOf(computer), cloudComputers = listOf(old, replacement),
            connection = LinkState.Failed("Access revoked.", 403))
        var asked: CloudComputer? = null
        compose.setContent { Hosted { SettingsView(state, viewModel(), null, {}, { asked = it }, {}) } }
        compose.onAllNodesWithText("Ask for access")[1].assertIsEnabled().performClick()
        assertEquals(replacement, asked)
        compose.onNodeWithText("Remove this phone's computer to use this one", substring = true).assertDoesNotExist()
    }

    @Test fun anExistingPairingDoesNotPretendANewRequestWasApproved() {
        val cloud = CloudComputer(computer.id, computer.name, computer.signKey, online = true, access = "granted")
        val state = mutableStateOf(fixture().copy(computers = listOf(computer), cloudComputers = listOf(cloud),
            access = AccessTicket("new-request", Long.MAX_VALUE, "123456", computer.id, computer.signKey), busy = setOf("access")))
        compose.setContent { Hosted { AccessRequestView(state.value, viewModel(), cloud) } }
        compose.onNodeWithText("123 456").assertIsDisplayed()
        compose.onNodeWithText("${computer.name} approved this phone").assertDoesNotExist()
        compose.runOnUiThread { state.value = state.value.copy(access = null, busy = emptySet(), approvedAccess = mapOf(computer.id to "new-request")) }
        compose.onNodeWithText("${computer.name} approved this phone").assertIsDisplayed()
    }

    @Test fun existingApprovalRequiresAnExplicitRenewal() {
        val cloud = CloudComputer(computer.id, computer.name, computer.signKey, online = true, access = "granted")
        compose.setContent { Hosted { AccessRequestView(fixture().copy(computers = listOf(computer)), viewModel(), cloud) } }
        compose.onNodeWithText("Request new approval").assertIsDisplayed()
        compose.onNodeWithText("${computer.name} approved this phone").assertDoesNotExist()
        compose.onNodeWithText("Waiting for approval…").assertDoesNotExist()
    }

    @Test fun anApprovedHandoffWaitsForAnActiveSessionOperation() {
        lateinit var store: AppStore
        val release = CompletableDeferred<Unit>()
        val calls = mutableListOf<String>()
        compose.setContent { store = viewModel() }
        compose.runOnUiThread {
            store.sessionAction { calls.add("first"); release.await(); calls.add("finished") }
            store.sessionAction(wait = true) { calls.add("approved") }
        }
        compose.runOnIdle { assertEquals(listOf("first"), calls) }
        compose.runOnUiThread { release.complete(Unit) }
        compose.waitUntil { calls.size == 3 }
        compose.runOnIdle { assertEquals(listOf("first", "finished", "approved"), calls) }
    }

    @Test fun approvalRemainsVisibleWhileTheComputerHandoffWaits() {
        lateinit var store: AppStore
        val release = CompletableDeferred<Unit>()
        compose.setContent { store = viewModel() }
        compose.waitUntil { !store.state.value.loading }
        compose.runOnUiThread {
            store.mutableState.value = store.state.value.copy(computers = listOf(computer),
                access = AccessTicket("approved-request", Long.MAX_VALUE, "123456", "replacement", "new-key"),
                busy = setOf("access"))
            store.sessionAction { release.await() }
            store.useApprovedComputer(Computer("replacement", "New computer", "new-key"), "approved-request")
            // The access polling action ends as soon as the cloud confirms approval.
            store.mutableState.value = store.state.value.copy(access = null, busy = store.state.value.busy - "access")
        }
        compose.runOnIdle {
            assertEquals("approved-request", store.state.value.approvedAccess["replacement"])
            assertEquals(computer, store.state.value.computers.single())
        }
        // Retire this test's generation so releasing the queue cannot persist a pairing.
        compose.runOnUiThread {
            store.mutableState.value = store.state.value.copy(generation = store.state.value.generation + 1)
            release.complete(Unit)
        }
        compose.waitUntil { "session" !in store.state.value.busy }
    }

    @Test fun explicitRenewalReplacesANegotiatingAccessAction() {
        lateinit var store: AppStore
        var cancelled = false
        val cleanupStarted = CompletableDeferred<Unit>()
        val releaseCleanup = CompletableDeferred<Unit>()
        compose.setContent { store = viewModel() }
        compose.waitUntil { !store.state.value.loading }
        val target = CloudComputer(computer.id, computer.name, computer.signKey, online = true, access = "granted")
        compose.runOnUiThread {
            store.cloud = null
            store.action("access") {
                try { awaitCancellation() } finally {
                    withContext(NonCancellable) {
                        cleanupStarted.complete(Unit)
                        releaseCleanup.await()
                        cancelled = true
                    }
                }
            }
            store.requestAccess(target)
        }
        compose.runOnIdle { assertFalse(cancelled) } // Ordinary repeated asks don't replace it.
        compose.runOnUiThread { store.requestAccess(target, renew = true) }
        compose.waitUntil { cleanupStarted.isCompleted }
        compose.runOnIdle {
            assertFalse(cancelled)
            assertTrue("renewAccess" in store.state.value.busy)
            assertNull(store.state.value.error) // Replacement hasn't reached the missing-cloud check.
        }
        compose.runOnUiThread { releaseCleanup.complete(Unit) }
        compose.waitUntil { cancelled && "access" !in store.state.value.busy && "renewAccess" !in store.state.value.busy }
        compose.runOnIdle {
            // The replacement actually ran; merely cancelling the old job cannot produce this error.
            assertEquals("Sign in to reach your account.", store.state.value.error)
            assertNull(store.state.value.askingAccess)
        }
    }

    private fun snapshot(name: String) {
        if (InstrumentationRegistry.getArguments().getString("layoutSnapshots") != "true") return
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        val file = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "account-$name.png")
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }
}
