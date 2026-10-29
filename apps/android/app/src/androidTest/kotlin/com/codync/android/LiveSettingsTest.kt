package com.codync.android

import android.os.Build
import android.view.KeyEvent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.util.UUID

/** A disposable bot on the isolated fake-agent host; no actual agent work is started. */
class LiveSettingsTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    @Test fun botSettingsSaveAHostFolderAndTurnDefaultNotificationsOff() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("liveHost") == "true")
        assumeTrue(Build.PRODUCT.startsWith("sdk"))
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertNull(PushPreferences.activeUser(context))
        val local = LocalStore(context)
        val computer = local.computers().single()
        requireFixtureHost(computer.id)
        val route = computer.copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly)
        val channel = runBlocking { HostConnector.connect(route, local.identity()) }
        var clone: String? = null
        try {
            val bots = runBlocking { HostClient.bots(channel) }
            val parent = bots.single { it.name == "Android chat acceptance" }
            val response = runBlocking { channel.call("createBot", BotDraft.from(parent).copy(id = null,
                name = "Settings acceptance " + UUID.randomUUID().toString().take(6), cwd = "/tmp", notify = null).body()) }
            val bot = HostClient.decode(response.jsonObject.getValue("bot").jsonObject)
            clone = bot.id
            val listingResponse = runBlocking { channel.call("listDirs", body("path" to "/tmp")) }
            val listing = WireJson.decodeFromJsonElement<DirListing>(listingResponse)
            val store = ViewModelProvider(compose.activity)[AppStore::class.java]
            compose.waitUntil(30_000) { !store.state.value.loading && store.state.value.bots.any { it.id == bot.id } && store.state.value.actualConnection is LinkState.Ready }
            compose.runOnUiThread { store.openBot(null) }
            compose.rosterAction(bot.name, "Edit profile")
            compose.onNode(hasSetTextAction() and hasText(bot.name)).performTextReplacement("Unsaved draft")
            compose.onNodeWithContentDescription("Workspace").performScrollTo().performClick()
            compose.onNodeWithText("Choose project folder…").performClick()
            val useFolder = "Use “${listing.path.trimEnd('/').substringAfterLast('/').ifEmpty { "/" }}”"
            compose.waitUntil(15_000) { compose.onAllNodesWithText(useFolder).fetchSemanticsNodes().size == 1 }
            InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(KeyEvent.KEYCODE_BACK)
            compose.waitUntil(5_000) { compose.onAllNodesWithText(useFolder).fetchSemanticsNodes().isEmpty() }
            compose.onNode(hasSetTextAction() and hasText("Unsaved draft")).performScrollTo().assertIsDisplayed()
            compose.onNodeWithContentDescription("Close").performClick()
            compose.waitUntil(5_000) { compose.onAllNodesWithText("Settings").fetchSemanticsNodes().isEmpty() }
            val cancelledBots = runBlocking { HostClient.bots(channel) }
            val cancelled = cancelledBots.single { it.id == bot.id }
            assertEquals(bot.name, cancelled.name)
            assertEquals(bot.cwd, cancelled.cwd)
            assertNull(cancelled.notify)
            compose.rosterAction(bot.name, "Edit profile")
            compose.onNodeWithContentDescription("Workspace").performScrollTo().performClick()
            compose.onNodeWithText("Choose project folder…").performClick()
            compose.waitUntil(15_000) { compose.onAllNodesWithText(useFolder).fetchSemanticsNodes().size == 1 }
            compose.onNodeWithText(useFolder).performClick()
            compose.waitUntil(5_000) { compose.onAllNodesWithText(useFolder).fetchSemanticsNodes().isEmpty() }
            compose.onNodeWithContentDescription("Workspace").assert(hasText(listing.path.trimEnd('/').substringAfterLast('/')))
            compose.onNode(hasText("Notifications") and isToggleable()).performScrollTo().assertIsOn().performClick().assertIsOff()
            compose.onNodeWithContentDescription("Save").performClick()
            compose.waitUntil(15_000) { store.state.value.bots.firstOrNull { it.id == bot.id }?.let { it.notify == false && it.cwd == listing.path } == true && "editor" !in store.state.value.busy }
            val finalBots = runBlocking { HostClient.bots(channel) }
            val saved = finalBots.single { it.id == bot.id }
            assertEquals(listing.path, saved.cwd)
            assertEquals(false, saved.notify)
            assertEquals(parent.command, saved.command)
            assertEquals(parent.backend, saved.backend)
            assertEquals(parent.connectors, saved.connectors)
            assertEquals(parent.skills, saved.skills)
            assertNull(store.state.value.error)
        } finally {
            clone?.let { id -> runBlocking { channel.call("deleteBot", body("botId" to id)) } }
            channel.close()
        }
    }
}
