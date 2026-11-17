package com.codync.android

import com.codync.android.core.Computer
import com.codync.android.core.Entry
import com.codync.android.design.MenuItem
import com.codync.android.design.ReactionPick
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.assertEquals
import org.junit.Test

class MessageMenuTest {
    @Test fun `a picture's message menu has Copy image and Copy name where Copy was`() {
        val menu = MessageMenu(ReactionPick(emptyList(), emptyList()) {}, reply = {}, trace = {})
        assertEquals(listOf("Copy image", "Copy name", "Reply", "Show what it did"), menu.items(imageCopyItems({}, {})).map { it.title })
    }

    @Test fun `in the viewer Reply and Show what it did close it first`() {
        val done = mutableListOf<String>()
        val menu = MessageMenu(ReactionPick(emptyList(), emptyList()) {}, reply = { done += "reply" }, trace = { done += "trace" })
        val items = menu.items(imageCopyItems({ done += "copy" }, {}), leave = { done += "close" })
        items.forEach { it.action() }
        assertEquals(listOf("copy", "close", "reply", "close", "trace"), done)
    }

    @Test fun `a message's text offers Select Text right after Copy`() {
        val opened = mutableListOf<String>()
        val menu = MessageMenu(ReactionPick(emptyList(), emptyList()) {}, reply = {}, trace = {}, selectText = { opened += "select" })
        val items = menu.textItems(MenuItem("Copy") {}, listOf(MenuItem("Download") {}))
        assertEquals(listOf("Copy", "Select Text", "Download", "Reply", "Show what it did"), items.map { it.title })
        items[1].action()
        assertEquals(listOf("select"), opened)
        // A picture's menu has none, nor a row that doesn't offer it.
        assertEquals(listOf("Copy image", "Copy name", "Reply", "Show what it did"), menu.items(imageCopyItems({}, {})).map { it.title })
        val plain = MessageMenu(ReactionPick(emptyList(), emptyList()) {}, reply = null, trace = null)
        assertEquals(listOf("Copy"), plain.textItems(MenuItem("Copy") {}).map { it.title })
    }

    @Test fun `Select Text goes where Reply does on a message with text of its own`() {
        fun entry(text: String, artifact: Boolean = false) = Entry("e", 1, "b", rev = 1, kind = "agent", data = buildJsonObject {
            put("text", text); put("final", true)
            if (artifact) put("artifact", buildJsonObject { put("id", "a1"); put("title", "Plan") })
        })
        assertTrue(MessageMenu.offersSelectText(entry("Step one."), replyable = true))
        assertFalse(MessageMenu.offersSelectText(entry("Step one."), replyable = false))
        assertFalse(MessageMenu.offersSelectText(entry("  "), replyable = true))
        assertFalse(MessageMenu.offersSelectText(entry("Plan", artifact = true), replyable = true))
    }

    @Test fun `Select Text waits for a host that takes a part`() {
        fun host(version: String?) = AppState(Computer("fixture", "Studio", "fixture"), hello = version?.let { buildJsonObject { put("version", it) } })
        assertFalse(host("2.48.0").supportsReplyPart)
        assertTrue(host("2.48.1").supportsReplyPart)
        assertFalse(host(null).supportsReplyPart)
    }

    @Test fun `a sent message has no Show what it did`() {
        val menu = MessageMenu(ReactionPick(emptyList(), emptyList()) {}, reply = {}, trace = null)
        assertEquals(listOf("Copy image", "Copy name", "Reply"), menu.items(imageCopyItems({}, {})).map { it.title })
    }
}
