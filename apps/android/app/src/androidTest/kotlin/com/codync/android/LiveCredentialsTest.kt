package com.codync.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.util.UUID

/** Opt-in, account-free, disposable host only. The caller seeds requests through its local token. */
class LiveCredentialsTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun savedSignInsAndImportedConnectorCredentialsStayOnTheHost() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("liveCredentials") == "true")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertNull(PushPreferences.activeUser(context))
        val local = LocalStore(context)
        val computer = local.computers().single()
        requireFixtureHost(computer.id)
        val route = computer.copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly)
        val channel = runBlocking { HostConnector.connect(route, local.identity()) }
        val original = runBlocking { HostClient.bots(channel) }
        val suffix = UUID.randomUUID().toString().take(8)
        val site = "android-$suffix.fixture.invalid"
        val name = "Android credentials $suffix"
        try {
            val store = compose.computerStore()
            compose.waitUntil(30_000) { !store.state.value.loading && store.state.value.actualConnection is LinkState.Ready }
            compose.runOnUiThread { store.openBot(null) }
            compose.openExtensions()
            compose.onNodeWithText("Credentials").performScrollTo().performClick()
            field("github.com").performTextReplacement(site)
            field("name@example.com").performTextReplacement("fixture-user")
            field("Password or op:// reference").performTextReplacement("  fixture-password  ")
            compose.onNodeWithText("Save sign-in").performScrollTo().performClick()
            compose.waitUntil(20_000) { listedLogins(channel).any { it.site == site } }
            val saved = listedLogins(channel).single { it.site == site }
            assertEquals("fixture-user", saved.username)
            assertFalse(WireJson.encodeToJsonElement(saved).toString().contains("fixture-password"))
            compose.waitUntil(20_000) { compose.onAllNodesWithText(site).fetchSemanticsNodes().size == 1 }
            compose.onRowButton("Remove", site).performClick()
            compose.waitUntil(20_000) { listedLogins(channel).none { it.id == saved.id } }
            back()
            // Paste an MCP config into a custom connector: every server in it is added.
            compose.onNodeWithText("Custom connector").performScrollTo().performClick()
            compose.onNodeWithText("Config").performClick()
            val fixturePath = requireNotNull(InstrumentationRegistry.getArguments().getString("credentialMcp"))
            assertTrue(fixturePath.endsWith("/Codync/host/tests/fixtures/credential_mcp.py"))
            val config = buildJsonObject { putJsonObject("mcpServers") { putJsonObject(name) {
                put("command", "/opt/homebrew/bin/python3.13"); put("args", JsonArray(listOf(JsonPrimitive(fixturePath))))
                putJsonObject("env") { put("API_KEY", "test-private-key"); put("PRESERVED", "fixture-preserved") }
            } } }.toString()
            field("{ \"mcpServers\": { … } }").performTextReplacement(config)
            compose.onNodeWithContentDescription("Add").performClick()
            compose.waitUntil(30_000) { compose.onAllNodesWithText("Custom connector", useUnmergedTree = true).fetchSemanticsNodes().size == 1 }
            compose.waitUntil(30_000) { runBlocking { channel.call("connectors") }.toString().contains(name) }
            compose.onNodeWithText("Credentials").performScrollTo().performClick()
            compose.waitUntil(20_000) { compose.onAllNodesWithText(name).fetchSemanticsNodes().isNotEmpty() }
            compose.onAllNodesWithText(name).onLast().performScrollTo().performClick()
            field("API_KEY").performTextReplacement("test-private-key")
            compose.onNodeWithContentDescription("Save credentials").performClick()
            compose.waitUntil(20_000) { compose.onAllNodesWithContentDescription("Save credentials").fetchSemanticsNodes().isEmpty() }
            val listing = runBlocking { channel.call("connectors") }
            val connector = WireJson.decodeFromJsonElement<List<InstalledConnector>>(listing.jsonObject.getValue("items")).single { it.name == name }
            assertEquals(setOf("API_KEY", "PRESERVED"), connector.keys.toSet())
            assertFalse(listing.toString().contains("test-private-key"))
            assertFalse(listing.toString().contains("fixture-preserved"))
            assertTrue(runCatching { runBlocking { channel.call("connectorRuntime", body("id" to connector.id)) } }.isFailure)
        } finally {
            runBlocking {
                val logins = channel.call("credentialLogins")
                for (login in WireJson.decodeFromJsonElement<List<SavedLogin>>(logins.jsonObject.getValue("items")).filter { it.site == site })
                    channel.call("credentialRemoveLogin", body("id" to login.id))
                val plugins = channel.call("connectors")
                for (connector in WireJson.decodeFromJsonElement<List<InstalledConnector>>(plugins.jsonObject.getValue("items")).filter { it.name == name })
                    channel.call("removeConnector", body("id" to connector.id))
                val remaining = HostClient.bots(channel)
                for (bot in original.filter { previous -> remaining.any { it.id == previous.id } }) channel.call("updateBot", buildJsonObject {
                    put("id", bot.id); put("connectors", WireJson.encodeToJsonElement(bot.connectors)); put("skills", WireJson.encodeToJsonElement(bot.skills))
                })
            }
            channel.close()
        }
    }

    @Test fun aLoginCardSavesWithoutPuttingThePasswordInChat() {
        val args = InstrumentationRegistry.getArguments()
        val botId = args.getString("credentialBot")
        assumeTrue(botId != null && args.getString("liveCredentials") == "true")
        val savedRequest = requireNotNull(args.getString("loginRequest"))
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertNull(PushPreferences.activeUser(context))
        val local = LocalStore(context)
        val computer = local.computers().single()
        requireFixtureHost(computer.id)
        val route = computer.copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly)
        val channel = runBlocking { HostConnector.connect(route, local.identity()) }
        try {
            val store = compose.computerStore()
            compose.waitUntil(30_000) { store.state.value.bots.any { it.id == botId } && store.state.value.entries.any { it.id == savedRequest } }
            assertTrue(store.state.value.bots.single { it.id == botId }.name.startsWith("Android credentials acceptance"))
            compose.runOnUiThread { store.openBot(botId) }
            cardField(savedRequest, "Username or email").performTextReplacement("fixture-card-user")
            cardField(savedRequest, "Password or op:// reference").performTextReplacement("fixture-card-password")
            cardAction(savedRequest, "Save").performClick()
            compose.waitUntil(20_000) { store.state.value.entries.any { it.id == savedRequest && it.data["connectionRequest"]?.jsonObject?.get("status")?.jsonPrimitive?.content == "ready" } }
            compose.onNode(hasText("Saved") and hasAnyAncestor(hasTestTag("connection:$savedRequest"))).assertIsDisplayed()
            runBlocking { store.finishConnectionRequest(savedRequest, value = "fixture-card-password", username = "fixture-card-user") }
            val history = runBlocking { channel.call("history", body("botId" to botId)) }
            val entries = history.jsonObject.getValue("entries").jsonArray
            assertEquals(1, entries.count { it.jsonObject["id"]?.jsonPrimitive?.content == "connection-ack-$savedRequest" })
            assertFalse(history.toString().contains("fixture-card-password"))
            val publicLogins = runBlocking { channel.call("credentialLogins") }
            assertFalse(publicLogins.toString().contains("fixture-card-password"))
        } finally { channel.close() }
    }

    private fun listedLogins(channel: EncryptedChannel): List<SavedLogin> = runBlocking {
        val response = channel.call("credentialLogins")
        WireJson.decodeFromJsonElement(response.jsonObject.getValue("items"))
    }
    /** Extensions fields are labeled by their prompt. */
    private fun field(prompt: String): SemanticsNodeInteraction = compose.onNode(hasSetTextAction() and hasContentDescription(prompt)).performScrollTo()
    private fun cardField(id: String, prompt: String): SemanticsNodeInteraction = compose.onNode(
        hasSetTextAction() and hasContentDescription(prompt) and hasAnyAncestor(hasTestTag("connection:$id"))).performScrollTo()
    private fun back() = InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK)
    private fun cardAction(id: String, label: String): SemanticsNodeInteraction = compose.onNode(
        hasClickAction() and hasText(label) and hasAnyAncestor(hasTestTag("connection:$id"))).performScrollTo()
}
