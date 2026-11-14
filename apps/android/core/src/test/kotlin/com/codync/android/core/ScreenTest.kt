package com.codync.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class ScreenTest {
    private val display = ScreenDisplay(7, "Studio Display", 1600.0, 1000.0, main = true)

    @Test fun `screen state decodes what older and newer hosts send and explains setup`() {
        val state = ScreenState.decode(WireJson.parseToJsonElement("""{"enabled":true,"connected":true,"platform":"macos","capture":true,
            "displays":[{"id":1,"name":"Side","width":800,"height":600,"main":false},{"id":7,"name":"Studio Display","width":1600,"height":1000,"main":true}],
            "agentBot":"b1","future":1}""").jsonObject)
        assertTrue(state.available)
        assertFalse(state.controlReady)
        assertEquals(7L, state.mainDisplay?.id)
        assertEquals("b1", state.agentBot)
        assertTrue(state.accessGuidance.startsWith("Allow computer control for Codync Screen."))
        assertEquals(ScreenState(), ScreenState.decode(buildJsonObject {}))
        assertTrue(ScreenState(enabled = true, connected = true, platform = "linux", capture = true).accessGuidance.startsWith("Choose a screen"))
        assertTrue(ScreenState(platform = "windows", computerUse = true).controlReady)
        assertTrue(ScreenState().accessGuidance.startsWith("Remote screen is off."))
    }

    @Test fun `replies and requests match the data channel protocol`() {
        assertEquals(ScreenReply.Clipboard("hi"), ScreenReply.decode("""{"type":"clipboard","text":"hi"}"""))
        assertEquals(ScreenReply.View(ScreenRect(1.0, 2.0, 3.0, 4.0)), ScreenReply.decode("""{"type":"view","crop":[1,2,3,4]}"""))
        assertEquals(ScreenReply.View(null), ScreenReply.decode("""{"type":"view"}"""))
        assertEquals(ScreenReply.Error("nope"), ScreenReply.decode("""{"type":"error","message":"nope"}"""))
        assertNull(ScreenReply.decode("not json"))
        assertNull(ScreenReply.decode("""{"type":"later"}"""))
        assertEquals("""{"type":"view","long":2400.0,"short":1080.0,"display":7,"crop":[0.0,0.0,800.0,500.0]}""",
            ScreenRequests.view(7, ScreenRect(0.0, 0.0, 800.0, 500.0), ScreenSize(1080.0, 2400.0)).toString())
        assertEquals("""{"type":"view","long":10.0,"short":5.0}""", ScreenRequests.view(null, null, ScreenSize(10.0, 5.0)).toString())
        assertEquals("""{"type":"view","display":2}""", ScreenRequests.switchDisplay(2).toString())
    }

    @Test fun `hardware H264 High leads and renewal leaves five minutes`() {
        val codecs = listOf("VP8" to null, "H264" to "42e01f", "AV1" to null, "H264" to "640c1f")
        assertEquals(listOf("H264" to "640c1f", "H264" to "42e01f", "VP8" to null, "AV1" to null),
            preferredScreenCodecs(codecs, { it.first }, { it.second }))
        val connection = ScreenConnection("s", emptyList(), expiresAt = 3_600_000)
        assertEquals(3_300_000, connection.renewIn(0))
        assertEquals(1_000, connection.renewIn(3_500_000))
    }

    @Test fun `viewport zooms around the fingers, stays on the display and streams the visible part`() {
        var v = ScreenViewport(ScreenSize(1600.0, 1000.0), ScreenSize(400.0, 800.0))
        assertEquals(0.25, v.scale, 1e-9)
        assertNull(v.crop)
        assertEquals(ScreenRect(0.0, 275.0, 400.0, 250.0), v.frame(v.displayBounds))
        val anchor = ScreenPoint(100.0, 400.0)
        val before = v.toDisplay(anchor)
        v = v.zoomed(4.0, anchor)
        assertEquals(before.x, v.toDisplay(anchor).x, 1e-9)
        assertEquals(ScreenRect(300.0, 100.0, 400.0, 800.0), v.crop)
        // Pans stop at the display's edge.
        assertEquals(ScreenRect(1200.0, 100.0, 400.0, 800.0), v.panned(ScreenPoint(-10_000.0, 0.0)).crop)
        assertEquals(16.0, v.maxZoom, 1e-9)
        assertEquals(16.0, v.zoomed(100.0, anchor).zoom, 1e-9)
        assertNull(v.reset().crop)
        // A pointer near the edge pulls the view along.
        assertTrue(v.following(ScreenPoint(1599.0, 500.0)).center.x > v.center.x)
        assertEquals(v, v.following(v.center))
    }

    @Test fun `trackpad moves the pointer faster with the finger and taps click there`() {
        val sent = mutableListOf<JsonObject>()
        val control = ScreenControl { sent += it }
        control.setViewSize(ScreenSize(400.0, 250.0))
        control.setDisplay(display)
        control.onePan(ScreenPoint(10.0, 0.0), speed = 0.0)
        assertEquals(840.0, sent.last().double("x"), 1e-9)
        control.onePan(ScreenPoint(10.0, 0.0), speed = 1800.0)
        assertEquals(920.0, sent.last().double("x"), 1e-9)
        control.tap(ScreenPoint(0.0, 0.0), 1_000)
        control.tap(ScreenPoint(0.0, 0.0), 1_200)
        control.tap(ScreenPoint(0.0, 0.0), 1_400)
        control.tap(ScreenPoint(0.0, 0.0), 1_600)
        assertEquals(listOf(1, 2, 3, 3), sent.takeLast(4).map { it.getValue("count").jsonPrimitive.int })
        assertEquals(920.0, sent.last().double("x"), 1e-9)
        control.tap(ScreenPoint(0.0, 0.0), 3_000)
        assertEquals(1, sent.last().getValue("count").jsonPrimitive.int)
    }

    @Test fun `direct taps land under the finger, two fingers scroll naturally or pan while pinching`() {
        val sent = mutableListOf<JsonObject>()
        val control = ScreenControl { sent += it }
        control.mode = TouchMode.Direct
        control.setViewSize(ScreenSize(400.0, 250.0))
        control.setDisplay(display)
        control.tap(ScreenPoint(100.0, 50.0), 0)
        assertEquals(400.0 to 200.0, sent.last().double("x") to sent.last().double("y"))
        control.twoFingerTap(ScreenPoint(400.0, 250.0))
        assertEquals("right", sent.last().text("button"))
        assertEquals(1599.0, sent.last().double("x"), 1e-9)
        control.twoPan(ScreenPoint(0.0, 10.0), ScreenPoint(0.0, 0.0))
        assertEquals("scroll", sent.last().text("type"))
        assertEquals(-15.0, sent.last().double("dy"), 1e-9)
        val count = sent.size
        control.pinchBegan()
        control.pinch(2.0, ScreenPoint(200.0, 125.0))
        control.twoPan(ScreenPoint(10.0, 0.0), ScreenPoint(200.0, 125.0))
        control.pinchEnded()
        assertEquals("pinching only moves the view", count, sent.size)
        assertTrue(control.zoomed)
    }

    @Test fun `a drag holds the button until the finger lifts, even after handing control back`() {
        val sent = mutableListOf<JsonObject>()
        val control = ScreenControl { sent += it }
        control.setViewSize(ScreenSize(400.0, 250.0))
        control.setDisplay(display)
        assertTrue(control.pressBegan(ScreenPoint(10.0, 10.0)))
        control.pressMoved(ScreenPoint(20.0, 10.0))
        control.interactive = false
        control.pressEnded()
        assertEquals(listOf("down", "move", "up"), sent.map { it.text("type") })
        assertFalse(control.pressBegan(ScreenPoint(0.0, 0.0)))
        control.tap(ScreenPoint(0.0, 0.0), 0)
        control.typed("x")
        assertEquals(3, sent.size)
    }

    @Test fun `armed modifiers apply once, to the next click or key`() {
        val sent = mutableListOf<JsonObject>()
        var consumed = 0
        val control = ScreenControl { sent += it }
        control.onArmedConsumed = { consumed++ }
        control.armed = listOf("cmd")
        control.typed("C")
        assertEquals("""{"type":"key","key":"c","modifiers":["cmd"]}""", sent.last().toString())
        control.typed("hi")
        assertEquals("""{"type":"text","text":"hi"}""", sent.last().toString())
        control.armed = listOf("shift")
        control.pressKey("tab", listOf("shift", "ctrl"))
        assertEquals(listOf("shift", "ctrl"), sent.last().getValue("modifiers").jsonArray.map { it.jsonPrimitive.content })
        control.armed = listOf("option")
        control.tap(ScreenPoint(0.0, 0.0), 0)
        assertEquals(listOf("option"), sent.last().getValue("modifiers").jsonArray.map { it.jsonPrimitive.content })
        assertEquals(3, consumed)
        assertTrue(control.armed.isEmpty())
    }

    private fun JsonObject.double(key: String) = getValue(key).jsonPrimitive.double
}
