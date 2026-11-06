package com.codync.android.core

import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Test

class CustomIconTest {
    @Test fun iconsEventsPreserveOriginalColorsAndWidgetImages() {
        val event = MirrorEvent.decode(WireJson.parseToJsonElement("""{"type":"icons","icons":{"bot:b":{"dataUrl":"data:image/png;base64,iVBORw0KGgo=","tinted":false}}}""").jsonObject) as MirrorEvent.Icons
        val icon = event.icons.getValue("bot:b")
        assertFalse(icon.tinted)
        val bots = widgetBots(listOf(Bot("b", "Scout", customIcon = icon)))
        assertEquals(icon, bots.single().customIcon)
        val cleared = MirrorEvent.decode(WireJson.parseToJsonElement("""{"type":"icons","icons":{}}""").jsonObject) as MirrorEvent.Icons
        assertTrue(cleared.icons.isEmpty())
    }

    @Test fun cachedNotificationIconsPreserveFreshNamesAndClearRemovedImages() {
        val icon = CustomIcon("data:image/png;base64,iVBORw0KGgo=", true)
        val fresh = listOf(Bot("b", "New name", customIcon = icon), Bot("member", "Speaker"))
        val enriched = fresh.withCustomIcons(mapOf("bot:member" to icon))
        assertEquals("New name", enriched[0].name)
        assertNull(enriched[0].customIcon)
        assertEquals(icon, enriched[1].customIcon)
        val alert = PushAlert("title", body = "body", from = "member", faces =
            listOf(Bot("group", "Team", kind = "group", members = listOf("member"))) + enriched)
        assertEquals(icon, alert.conversation("group")?.speaker?.customIcon)
    }

    @Test fun widgetFeedBudgetAcceptsAll128MaximumIconsAnd256BotSummaries() {
        val maximum = CustomIcon("data:image/png;base64," + "A".repeat(65_536), true)
        val bots = (0 until 256).map { Bot("bot-$it", "n".repeat(256), customIcon = maximum.takeIf { _ -> it < 128 }) }
        val bytes = WireJson.encodeToString(WidgetFeed(bots = widgetBots(bots))).toByteArray()
        assertTrue("The old limit would reject supported icons", bytes.size > 512 * 1024)
        assertTrue("All supported icons must fit", bytes.size <= MAX_WIDGET_FEED_BYTES)
    }
}
