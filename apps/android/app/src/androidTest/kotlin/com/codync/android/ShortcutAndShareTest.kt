package com.codync.android

import android.net.Uri
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.core.app.ApplicationProvider
import com.codync.android.core.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/**
 * A launcher shortcut's bot link and text shared from another app (iOS `LiveShortcutNavigationTests`):
 * which chat opens and where the shared text goes. Nothing links; the state says the computer is there.
 */
class ShortcutAndShareTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val mac = Computer("AAAAAAAAAAAAAAAAAAAAAA", "Mac fixture", "macos")
    private val link = "codync://bot/lead?scope=local&computer=AAAAAAAAAAAAAAAAAAAAAA"

    private fun store(): AppStore = AppStore(ApplicationProvider.getApplicationContext()).apply {
        draftBook = DraftBook(local.directory, mac.id)
        mutableState.update {
            it.copy(contextId = LocalStore.referenceId("local"), loading = false, rosterLoaded = true,
                computers = listOf(mac), account = AccountState(ready = true),
                connection = LinkState.Ready(HostRoute.Direct), actualConnection = LinkState.Ready(HostRoute.Direct),
                bots = listOf(Bot("lead", "Lead", lead = true), Bot("bot", "Bot")))
        }
    }

    @Test fun aShortcutsBotLinkOpensThatChat() = runBlocking(Dispatchers.Main) {
        val store = store()
        store.openLink(Uri.parse(link))
        withTimeout(10_000) { store.linkJob?.join() }
        assertEquals("lead", store.state.value.selectedBot)
        assertEquals(LinkPage.Bots, store.state.value.navigationPage)
    }

    @Test fun sharedTextGoesIntoThatBotsDraftAndOpensIt() {
        val store = store()
        runBlocking(Dispatchers.Main) { store.receiveShare("shared", emptyList(), botId = "bot") }
        compose.waitUntil(10_000) { store.state.value.selectedBot == "bot" }
        assertEquals("shared", store.state.value.drafts[ComposerDraft.lane("bot", null)]?.text)
        assertNull(store.state.value.incomingShare)
    }
}
