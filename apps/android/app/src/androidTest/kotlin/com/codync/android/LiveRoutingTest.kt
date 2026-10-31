package com.codync.android

import android.content.Intent
import android.net.Uri
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import com.codync.android.core.*
import java.io.File
import java.util.UUID
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.Rule

/** Controller OS-kills the emulator between phases. Never touches user identities or real bots. */
class LiveRoutingTest {
    @get:Rule val compose = createEmptyComposeRule()
    @Test fun coldNotificationFindsAnUncachedBotAndRejectsOtherContexts() {
        val phase = InstrumentationRegistry.getArguments().getString("routePhase")
        assumeTrue(phase in listOf("prepare", "restore"))
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        assertNull(PushPreferences.activeUser(context))
        val local = LocalStore(context)
        val computer = local.computers().single()
        requireFixtureHost(computer.id)
        val metadata = File(context.cacheDir, "route-acceptance.json")
        val route = computer.copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly)
        runBlocking {
            val channel = HostConnector.connect(route, local.identity())
            try {
                if (phase == "prepare") {
                    // No Activity owns a sync channel while we create this new fixture bot.
                    val bots = HostClient.bots(channel)
                    val parent = bots.single { it.name == "Android chat acceptance" }
                    val response = channel.call("createBot", BotDraft.from(parent).copy(id = null, name = "Routing acceptance " + UUID.randomUUID().toString().take(6)).body())
                    val bot = HostClient.decode(response.jsonObject.getValue("bot").jsonObject)
                    val cached = MirrorDatabase(context, local.directory, computer.id).use { it.snapshot().bots }
                    assertFalse(cached.any { it.id == bot.id })
                    metadata.writeText(buildJsonObject { put("bot", bot.id); put("parent", parent.id) }.toString())
                    return@runBlocking
                }
                val saved = Json.parseToJsonElement(metadata.readText()).jsonObject
                val bot = saved.getValue("bot").jsonPrimitive.content
                fun link(id: String = bot, scope: String = "local", cid: String = computer.id): Uri = Uri.Builder().scheme("codync").authority("bot").appendPath(id)
                    .appendQueryParameter("scope", scope).appendQueryParameter("computer", cid).build()
                try {
                    val intent = Intent(Intent.ACTION_VIEW, link(), context, MainActivity::class.java)
                    ActivityScenario.launch<MainActivity>(intent).use { scenario ->
                        var model: AccountStore? = null
                        scenario.onActivity { model = accountStore(it) }
                        val accounts = requireNotNull(model)
                        val store = withTimeout(35_000) {
                            accounts.state.first { !it.loading && it.contextId == "local" }
                            accounts.stores.first { stores -> stores.any { it.id == computer.id } }.single { it.id == computer.id }
                        }
                        withTimeout(35_000) { store.state.first { !it.loading && it.selectedBot == bot } }
                        assertNull(accounts.state.value.account.userId)
                        assertNull(accounts.state.value.error)
                        suspend fun reject(uri: Uri, message: String) {
                            instrumentation.runOnMainSync { accounts.openBot(null); accounts.clearError(); accounts.openLink(uri) }
                            withTimeout(15_000) { accounts.state.first { it.error?.contains(message) == true } }
                            assertNull(store.state.value.selectedBot)
                            assertEquals("local", accounts.state.value.contextId)
                            assertNull(accounts.state.value.account.userId)
                        }
                        reject(link(scope = "a".repeat(64)), "Switch to the account")
                        reject(link(cid = "AAAAAAAAAAAAAAAAAAAAAA"), "computer is no longer paired")
                        reject(link(id = "absent-" + UUID.randomUUID()), "bot is no longer available")
                        reject(Uri.parse(link().toString() + "&scope=local"), "invalid destination")
                        val parent = saved.getValue("parent").jsonPrimitive.content
                        // Usage opens from the State tab.
                        compose.onNode(hasText("State") and isSelectable()).performClick()
                        compose.onNodeWithContentDescription("Usage limits").performClick()
                        val usage = hasText("Usage") and isHeading()
                        compose.waitUntil(10_000) { compose.onAllNodes(usage).fetchSemanticsNodes().size == 1 }
                        instrumentation.runOnMainSync { accounts.openLink(link(id = parent)) }
                        withTimeout(10_000) { store.state.first { it.selectedBot == parent } }
                        assertNull(accounts.state.value.error)
                        compose.onNode(chatTitle).assertIsDisplayed()
                        // Recreating the Activity must retain the current navigation, not replay its original intent.
                        scenario.recreate()
                        scenario.onActivity { activity ->
                            val retained = accountStore(activity)
                            assertSame(accounts, retained)
                            assertSame(store, retained.store(computer.id))
                            assertEquals(parent, store.state.value.selectedBot)
                        }
                    }
                } finally { channel.call("deleteBot", body("botId" to bot)); metadata.delete() }
            } finally { channel.close() }
        }
    }
}
