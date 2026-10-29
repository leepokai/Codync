package com.codync.android

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

/** Opt-in, disposable fake-agent host only. Never execute this on a user's phone. */
class LiveManagementTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun groupsCreateMentionEditAndDeleteWithoutChangingTheirMembers() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("liveHost") == "true")
        val local = LocalStore(InstrumentationRegistry.getInstrumentation().targetContext)
        val computer = local.computers().single().copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly)
        val channel = runBlocking { HostConnector.connect(computer, local.identity()) }
        val created = mutableListOf<String>()
        try {
            val parent = runBlocking { HostClient.bots(channel) }.single { it.name == "Android chat acceptance" }
            val name = "Reviewer" + UUID.randomUUID().toString().take(6)
            val result = runBlocking { channel.call("createBot", BotDraft.from(parent).copy(id = null, name = name, permission = "auto").body()) }
            val member = HostClient.decode(result.jsonObject.getValue("bot").jsonObject)
            created.add(member.id)
            val store = ViewModelProvider(compose.activity)[AppStore::class.java]
            compose.waitUntil(30_000) { store.state.value.bots.any { it.id == member.id } }
            compose.runOnUiThread { store.openBot(null) }
            compose.onNodeWithContentDescription("New").performClick()
            compose.onNodeWithText("New group chat").performClick()
            // The member tiles, not the roster rows behind the sheet (those share the bot's name).
            compose.onNode(hasContentDescription(member.name) and isSelectable()).performScrollTo().performClick().assertIsSelected()
            compose.onNode(hasContentDescription(parent.name) and isSelectable()).performScrollTo().performClick().assertIsSelected()
            compose.onNodeWithContentDescription("Create").performClick()
            compose.waitUntil(30_000) { store.state.value.bots.any { it.id == store.state.value.selectedBot && it.kind == "group" } }
            val group = store.state.value.bots.single { it.id == store.state.value.selectedBot }
            created.add(group.id)
            assertEquals(listOf(member.id, parent.id), group.members)
            compose.onNodeWithText("New session").assertDoesNotExist()
            compose.onNode(hasSetTextAction()).performTextReplacement("@${member.name} Check the group lane")
            compose.onNodeWithContentDescription("Send").performClick()
            compose.waitUntil(30_000) { store.state.value.entries.any { it.botId == group.id && it.kind == "agent" && it.isChat } }
            val replies = store.state.value.entries.filter { it.botId == group.id && it.kind == "agent" && it.isChat }
            assertTrue(replies.isNotEmpty())
            assertTrue(replies.all { it.data["author"]?.jsonPrimitive?.content == member.id })
            // A group's title opens its editor.
            compose.openChatDetails()
            // The name field shows the members' names until the group has its own.
            compose.onNode(hasSetTextAction() and hasText(group.name.ifBlank { "${member.name}, ${parent.name}" })).performTextReplacement("Edited group")
            compose.onNodeWithContentDescription("Save").performClick()
            compose.waitUntil(30_000) { store.state.value.bots.firstOrNull { it.id == group.id }?.name == "Edited group" }
            assertEquals(group.members, store.state.value.bots.single { it.id == group.id }.members)
            compose.waitUntil(10_000) { compose.onAllNodes(chatTitle).fetchSemanticsNodes().size == 1 }
            compose.runOnUiThread { store.openBot(null) }
            compose.rosterActions("Edited group")
            // A group has no agent session of its own.
            compose.onNodeWithText("New session").assertDoesNotExist()
            compose.onNodeWithText("Pin").performScrollTo().performClick()
            compose.waitUntil(30_000) { store.state.value.bots.single { it.id == group.id }.pinned }
            compose.rosterActions("Edited group")
            compose.onNodeWithText("Hide from list").performScrollTo().performClick()
            compose.waitUntil(30_000) { store.state.value.bots.single { it.id == group.id }.hidden }
            // Hidden bots come back from Computer management.
            compose.computerAction("Manage computers")
            compose.onRowButton("Unhide", "Edited group").performClick()
            compose.waitUntil(30_000) { !store.state.value.bots.single { it.id == group.id }.hidden }
            InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(KeyEvent.KEYCODE_BACK)
            compose.waitUntil(10_000) { compose.onAllNodesWithText("Computer management").fetchSemanticsNodes().isEmpty() }
            compose.rosterActions("Edited group")
            compose.onNodeWithText("Edit group").performClick()
            compose.onNodeWithText("Delete group chat").performScrollTo().performClick()
            compose.onAllNodesWithText("Delete group chat").onLast().performClick()
            compose.waitUntil(30_000) { store.state.value.bots.none { it.id == group.id } && "delete:" + group.id !in store.state.value.busy }
            val remaining = runBlocking { HostClient.bots(channel) }
            assertFalse(remaining.any { it.id == group.id })
            assertTrue(remaining.any { it.id == member.id })
            assertTrue(remaining.any { it.id == parent.id })
            created.remove(group.id)
        } finally {
            runBlocking { for (id in created.asReversed()) channel.call("deleteBot", body("botId" to id)) }
            channel.close()
        }
    }
}
