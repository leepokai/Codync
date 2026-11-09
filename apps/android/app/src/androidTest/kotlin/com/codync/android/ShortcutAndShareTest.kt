package com.codync.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import com.codync.android.core.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/**
 * "Talk to Codync" and text shared from another app (iOS `LiveShortcutNavigationTests`): which chat opens, when its call
 * starts and when it never does, and where the text goes. Nothing links: the test says when a computer is connected.
 */
class ShortcutAndShareTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val mac = Computer("shortcut-test-mac-0001", "Mac fixture", "mac")
    private val linux = Computer("shortcut-test-linux-01", "Linux fixture", "linux")

    private fun connect(store: AppStore) = store.mutableState.update { it.copy(actualConnection = LinkState.Ready(HostRoute.Direct)) }

    @Test fun leavingTheChatDropsItsCallWhetherItWaitsForTheComputerOrForTheChat() = runBlocking(Dispatchers.Main) {
        for (openAnother in listOf(false, true)) {
            val store = previewStore(mac)
            fun leaveAndReturn() { store.openBot(if (openAnother) "other" else null); store.openBot("lead") }
            store.openBot("lead")
            store.requestCall("lead")
            leaveAndReturn()
            connect(store)
            store.callWait?.join()
            assertFalse(store.takeCallRequest("lead"))
            // A new one starts, once.
            store.requestCall("lead"); store.callWait?.join()
            assertTrue(store.takeCallRequest("lead"))
            assertFalse(store.takeCallRequest("lead"))
            // One the chat hasn't taken yet goes the same way.
            store.requestCall("lead"); store.callWait?.join()
            leaveAndReturn()
            assertFalse(store.takeCallRequest("lead"))
            store.openBot(null)
        }
    }

    @Test fun aComputerThatCantBeReachedIsAnErrorNotACall() = runBlocking(Dispatchers.Main) {
        val store = previewStore(mac)
        store.openBot("lead")
        store.requestCall("lead")
        withTimeout(25_000) { store.callWait?.join() }
        assertEquals("Can't reach the computer. Check that it's on, then try again.", store.state.value.error)
        assertFalse(store.takeCallRequest("lead"))
    }

    @Test fun talkToCodyncOpensThePickedLeadsChatAndCallsOnceItsComputerIsConnected() = compose.withPreviewAccounts(mac, linux) { accounts ->
        val previous = accounts.state.value.shortcutLeadComputer
        val store = requireNotNull(accounts.store(linux.id))
        try {
            runBlocking(Dispatchers.Main) {
                activate(accounts)
                requireNotNull(accounts.store(mac.id)).mutableState.update { it.copy(bots = listOf(Bot("lead-mac", "Mac lead", lead = true))) }
                store.mutableState.update { it.copy(bots = listOf(Bot("lead-linux", "Linux lead", lead = true))) }
                accounts.chooseShortcutLead(linux.id)
                accounts.talkToLead()
                withTimeout(10_000) { accounts.linkJob?.join() }
                assertEquals(LinkPage.Bots, accounts.state.value.navigationPage)
                assertEquals("lead-linux", store.state.value.selectedBot)
                assertNull(requireNotNull(accounts.store(mac.id)).state.value.selectedBot)
                assertNull(store.state.value.callRequest)
                connect(store); store.callWait?.join()
                assertEquals("lead-linux", store.state.value.callRequest)
                // Back drops a call the chat didn't take.
                accounts.openBot(null)
                assertNull(store.state.value.callRequest)
            }
        } finally { runBlocking(Dispatchers.Main) { accounts.chooseShortcutLead(previous) } }
    }

    @Test fun sharedTextGoesAfterTheTypedTextAndOpensThatChat() = compose.withPreviewAccounts(mac, linux) { accounts ->
        val store = requireNotNull(accounts.store(linux.id))
        runBlocking(Dispatchers.Main) {
            activate(accounts)
            store.editDraft("bot", "typed")
            accounts.share("shared")
            withTimeout(10_000) { accounts.linkJob?.join() }
            assertEquals("shared", accounts.state.value.sharing)
            accounts.sendShared(BotDestination(accounts.state.value.contextId, linux.id, "bot"))
            assertEquals("typed shared", store.state.value.drafts["bot"]?.text)
            assertEquals("bot", store.state.value.selectedBot)
            assertNull(accounts.state.value.sharing)
            accounts.openBot(null)
        }
    }

    /** The test activity doesn't start sign-in: the account it has is ready, as links wait for (`activated`). */
    private fun activate(accounts: AccountStore) = accounts.mutableState.update { it.copy(account = it.account.copy(ready = true)) }
}
