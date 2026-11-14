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

/** Installs inert fixture plugins only on the disposable host and restores its original bot choices. */
class LivePluginsTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    @Test fun customConnectorsAndSkillsInstallAndRemoveThroughTheRealHost() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("liveHost") == "true")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertNull(PushPreferences.activeUser(context))
        val local = LocalStore(context)
        val computer = local.computers().single()
        requireFixtureHost(computer.id)
        val route = computer.copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly)
        val channel = runBlocking { HostConnector.connect(route, local.identity()) }
        val original = runBlocking { HostClient.bots(channel) }
        var connectorId: String? = null; var skillId: String? = null
        val name = "Fixture " + UUID.randomUUID().toString().take(8)
        try {
            val store = compose.computerStore()
            compose.waitUntil(30_000) { !store.state.value.loading && store.state.value.actualConnection is LinkState.Ready }
            compose.runOnUiThread { store.openBot(null) }
            compose.openExtensions()
            compose.onNodeWithText("Custom connector").performScrollTo().performClick()
            field("Name").performTextReplacement(name + " connector")
            field("npx -y @scope/server").performTextReplacement("/usr/bin/false")
            field("API_KEY=…").performTextReplacement("FIXTURE_ONLY=fixture-value")
            compose.onNodeWithContentDescription("Add").assertIsEnabled().performClick()
            compose.waitUntil(20_000) { runBlocking { channel.call("connectors") }.toString().contains(name + " connector") }
            compose.onNodeWithText("Your own skill").performScrollTo().performClick()
            field("Name").performTextReplacement(name + " skill")
            field("When to use it").performTextReplacement("Disposable acceptance instructions")
            field("Step by step, in plain words…").performTextReplacement("This skill belongs to the isolated Android acceptance fixture.")
            compose.onNodeWithContentDescription("Save").assertIsEnabled().performClick()
            compose.runOnUiThread { store.refreshPlugins() }
            compose.waitUntil(20_000) { store.state.value.plugins?.let { plugins -> plugins.connectors.any { it.name == name + " connector" } &&
                plugins.skills.any { it.name == name + " skill" } } == true && "plugins" !in store.state.value.busy }
            val connector = store.state.value.plugins!!.connectors.single { it.name == name + " connector" }
            connectorId = connector.id
            assertTrue(connector.keys.contains("FIXTURE_ONLY"))
            assertEquals("local", connector.kind)
            assertFalse(WireJson.encodeToJsonElement(connector).toString().contains("fixture-value"))
            val skill = store.state.value.plugins!!.skills.single { it.name == name + " skill" }
            skillId = skill.id
            // Installed connectors and skills, each removable after a confirmation.
            compose.onNodeWithText("Connectors & skills").performScrollTo().performClick()
            compose.onNodeWithContentDescription("Remove " + skill.name).performScrollTo().performClick()
            compose.onNodeWithText("Remove ${skill.name}?").assertIsDisplayed()
            compose.onNodeWithText("Cancel").performClick()
            assertTrue(store.state.value.plugins!!.skills.any { it.id == skill.id })
            compose.onNodeWithContentDescription("Remove " + skill.name).performScrollTo().performClick()
            compose.onNodeWithText("Remove").performClick()
            compose.waitUntil(15_000) { store.state.value.plugins?.skills?.none { it.id == skill.id } == true }
            skillId = null
            compose.onNodeWithContentDescription("Remove " + connector.name).performScrollTo().performClick()
            compose.onNodeWithText("Remove").performClick()
            compose.waitUntil(15_000) { store.state.value.plugins?.connectors?.none { it.id == connector.id } == true }
            connectorId = null
            assertNull(store.state.value.error)
        } finally {
            runBlocking {
                // Find by fixture name as well, so an assertion before capturing the id still cleans up.
                val connectorResponse = channel.call("connectors")
                val connectors = WireJson.decodeFromJsonElement<List<InstalledConnector>>(connectorResponse.jsonObject.getValue("items"))
                for (connector in connectors.filter { it.id == connectorId || it.name == name + " connector" }) channel.call("removeConnector", body("id" to connector.id))
                val skillResponse = channel.call("skills")
                val skills = WireJson.decodeFromJsonElement<List<InstalledSkill>>(skillResponse.jsonObject.getValue("items"))
                for (skill in skills.filter { it.id == skillId || it.name == name + " skill" }) channel.call("removeSkill", body("id" to skill.id))
                // The existing host enables newly installed connectors on its bots.
                val remainingBots = HostClient.bots(channel)
                val remaining = remainingBots.map { it.id }.toSet()
                for (bot in original.filter { it.id in remaining }) channel.call("updateBot", buildJsonObject {
                    put("id", bot.id); put("connectors", WireJson.encodeToJsonElement(bot.connectors)); put("skills", WireJson.encodeToJsonElement(bot.skills))
                })
            }
            channel.close()
        }
    }
    /** Extensions fields are labeled by their prompt. */
    private fun field(prompt: String): SemanticsNodeInteraction = compose.onNode(hasSetTextAction() and hasContentDescription(prompt)).performScrollTo()
}
