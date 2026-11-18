package com.codync.android

import android.content.ClipData
import android.content.Intent
import android.net.Uri
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/**
 * "Talk to Codync" and what another app shares (iOS `LiveShortcutNavigationTests`): which chat opens, when its call
 * starts and when it never does, which files a share takes, and where it goes. Nothing links: the test says when a
 * computer is connected.
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

    @Test fun sharedTextGoesAsAMessageLeavesTheDraftAndOpensThatChatAsTheMostRecent() = compose.withPreviewAccounts(mac, linux) { accounts ->
        val store = requireNotNull(accounts.store(linux.id))
        val previous = accounts.state.value.shareRecipients
        try {
            runBlocking(Dispatchers.Main) {
                activate(accounts)
                store.editDraft("bot", "typed")
                val shared = SharedContent("shared", emptyList())
                accounts.share(shared)
                withTimeout(10_000) { accounts.linkJob?.join() }
                assertEquals(shared, accounts.state.value.sharing)
                accounts.sendShared(BotDestination(accounts.state.value.contextId, linux.id, "bot"), "shared, and a word", emptyList(),
                    shared.pasted("shared, and a word"), recent = false)
                val sent = store.state.value.pending.single { it.botId == "bot" }
                assertEquals("shared, and a word", sent.text)
                assertEquals(listOf("shared"), sent.pasted)
                assertEquals("typed", store.state.value.drafts["bot"]?.text)
                assertEquals("bot", store.state.value.selectedBot)
                assertEquals(ShareRecipient(linux.id, "bot"), accounts.state.value.shareRecipients.first())
                assertNull(accounts.state.value.sharing)
                accounts.openBot(null)
            }
        } finally {
            runBlocking(Dispatchers.Main) {
                accounts.mutableState.update { it.copy(shareRecipients = previous) }
                accounts.local.saveShareRecipients(previous)
            }
        }
    }

    @Test fun aShareTakesOtherAppsFilesEachOnceAtMostTenAndNoneOfCodyncsOwn() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val own = context.packageName
        fun foreign(n: Int) = Uri.parse("content://com.example.photos/$n")
        val refused = listOf(Uri.parse("content://$own.files/outgoing/secret"), Uri.parse("content://$own/secret"), Uri.parse("file:///sdcard/a.png"))
        val streams = listOf(foreign(0), foreign(0)) + refused + (1..11).map(::foreign)
        val multiple = Intent(Intent.ACTION_SEND_MULTIPLE).setType("image/*").putParcelableArrayListExtra(Intent.EXTRA_STREAM, ArrayList(streams))
            .putCharSequenceArrayListExtra(Intent.EXTRA_TEXT, arrayListOf<CharSequence>("one", "two"))
        val shared = SharedContent.from(multiple, own)
        assertEquals(SharedContent("one\ntwo", (0..9).map(::foreign)), shared)

        // One file; Android also copies a stream into the clip.
        val image = Intent(Intent.ACTION_SEND).setType("image/png").putExtra(Intent.EXTRA_STREAM, foreign(1)).apply { clipData = ClipData.newRawUri("", foreign(1)) }
        assertEquals(SharedContent("", listOf(foreign(1))), SharedContent.from(image, own))
        // A link whose clip is only its preview's thumbnail.
        val link = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, " https://example.com ")
            .apply { clipData = ClipData.newRawUri("", foreign(2)) }
        assertEquals(SharedContent("https://example.com", emptyList()), SharedContent.from(link, own))

        // Handed on to Codync's own task with the files' read grant, and read back the same there.
        val forwarded = shared.forward(context)
        assertEquals(MainActivity::class.java.name, forwarded.component?.className)
        assertTrue(forwarded.flags and Intent.FLAG_GRANT_READ_URI_PERMISSION != 0)
        assertEquals(shared, SharedContent.from(forwarded, own))
    }

    @Test fun aFileThatCantBeReadStopsTheShareAndLeavesNothingPending() = compose.withPreviewAccounts(mac, linux) { accounts ->
        val store = requireNotNull(accounts.store(linux.id))
        runBlocking(Dispatchers.Main) {
            Analytics.start(InstrumentationRegistry.getInstrumentation().targetContext)
            // The installed app's own setting, put back below.
            val sharing = Analytics.enabled
            try {
                activate(accounts)
                // Dropping the queue keeps this test's events from PostHog.
                Analytics.setEnabled(false); Analytics.setEnabled(true)
                val unreadable = Uri.parse("content://com.example.missing/1")
                val failure = runCatching {
                    accounts.sendShared(BotDestination(accounts.state.value.contextId, linux.id, "bot"), "look", listOf(unreadable), emptyList(), recent = false)
                }
                assertTrue(failure.isFailure)
                assertTrue(store.state.value.pending.none { it.text == "look" })
                val event = Analytics.pending.last { it.getValue("event").jsonPrimitive.content == "share_sent" }.getValue("properties").jsonObject
                assertEquals("failed", event.getValue("outcome").jsonPrimitive.content)
                assertEquals("read", event.getValue("error").jsonPrimitive.content)
            } finally { Analytics.setEnabled(false); Analytics.setEnabled(sharing) }
        }
    }

    /** The test activity doesn't start sign-in: the account it has is ready, as links wait for (`activated`). */
    private fun activate(accounts: AccountStore) = accounts.mutableState.update { it.copy(account = it.account.copy(ready = true)) }
}
