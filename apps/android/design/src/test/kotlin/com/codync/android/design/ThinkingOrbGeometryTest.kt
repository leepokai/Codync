package com.codync.android.design

import java.io.File
import kotlin.math.abs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.double
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ThinkingOrbGeometryTest {
    /** The fixture is written by the desktop's thinking-orb test and read by the iPhone's too. */
    @Test fun `frames match the shared fixture`() {
        val fixture = Json.parseToJsonElement(File(System.getProperty("codync.orbs")).readText()).jsonObject
        val tolerance = fixture.getValue("tolerance").jsonPrimitive.double
        val rest = fixture.getValue("rest").jsonObject
        for (state in ThinkingOrbState.entries) {
            assertEquals("${state.name} still pose", rest.getValue(state.name.lowercase()).jsonPrimitive.double, ThinkingOrbGeometry.rest(state), 0.0)
        }
        for (case in fixture.getValue("cases").jsonArray.map { it.jsonObject }) {
            val name = case.getValue("state").jsonPrimitive.content
            val size = case.getValue("size").jsonPrimitive.double
            val t = case.getValue("t").jsonPrimitive.double
            val expected = case.getValue("dots").jsonArray.map { dot -> dot.jsonArray.map { it.jsonPrimitive.double } }
            val dots = ThinkingOrbGeometry.frame(ThinkingOrbState.valueOf(name.replaceFirstChar(Char::uppercase)), size, t)
            assertEquals("$name $size $t dots", expected.size, dots.size)
            dots.zip(expected).forEachIndexed { index, (dot, values) ->
                listOf(dot.x, dot.y, dot.radius, dot.alpha).zip(values).forEach { (actual, value) ->
                    assertTrue("$name $size $t dot $index: $actual != $value", abs(actual - value) <= tolerance)
                }
            }
        }
    }

    @Test fun `dots stay put while their size changes`() {
        for (state in ThinkingOrbState.entries) {
            val start = ThinkingOrbGeometry.frame(state, 16.0, 0.0)
            val later = ThinkingOrbGeometry.frame(state, 16.0, 1.5)
            assertEquals(start.map { it.x to it.y }, later.map { it.x to it.y })
            assertNotEquals(state.name, start.map { it.radius }, later.map { it.radius })
        }
        assertEquals(listOf(21, 37, 69), listOf(13.0, 20.0, 32.0).map { ThinkingOrbGeometry.frame(ThinkingOrbState.Working, it, 0.0).size })
    }

    @Test fun `each state rests on a pose of its own`() {
        val poses = ThinkingOrbState.entries.map { state ->
            ThinkingOrbGeometry.frame(state, 16.0, ThinkingOrbGeometry.rest(state)).map { Math.round(it.radius * 1e4) }
        }
        assertEquals(ThinkingOrbState.entries.size, poses.toSet().size)
    }
}
