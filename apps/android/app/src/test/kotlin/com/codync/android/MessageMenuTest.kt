package com.codync.android

import com.codync.android.design.ReactionPick
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

    @Test fun `a sent message has no Show what it did`() {
        val menu = MessageMenu(ReactionPick(emptyList(), emptyList()) {}, reply = {}, trace = null)
        assertEquals(listOf("Copy image", "Copy name", "Reply"), menu.items(imageCopyItems({}, {})).map { it.title })
    }
}
