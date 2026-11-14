package com.codync.android.design

import java.io.File
import kotlin.math.abs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The port draws exactly what thinking-orbs' golden frames (shared with the iOS tests) hold. */
class ThinkingOrbGeometryTest {
    @Test fun `frames match the upstream golden vectors`() {
        val fixture = Json.parseToJsonElement(File(System.getProperty("codync.orbs")).readText()).jsonObject
        val tolerance = fixture.getValue("tolerance").jsonPrimitive.double
        for (case in fixture.getValue("cases").jsonArray.map { it.jsonObject }) {
            val key = case.getValue("key").jsonPrimitive.content
            val state = ThinkingOrbState.valueOf(case.getValue("state").jsonPrimitive.content.replaceFirstChar(Char::uppercase))
            val frame = ThinkingOrbGeometry.frame(state, case.getValue("size").jsonPrimitive.double, case.getValue("t").jsonPrimitive.double)
            assertEquals("$key dots", case.getValue("dotCount").jsonPrimitive.int, frame.dots.size)
            assertEquals("$key lines", case.getValue("lineCount").jsonPrimitive.int, frame.lines.size)
            for (sample in case.getValue("dots").jsonArray.map { it.jsonObject }) {
                val d = frame.dots.getOrNull(sample.getValue("index").jsonPrimitive.int) ?: continue
                listOf(d.x, d.y, d.z, d.radius, d.white, d.alpha).zip(sample.getValue("values").jsonArray.map { it.jsonPrimitive.double })
                    .forEach { (actual, expected) -> assertTrue("$key dot: $actual != $expected", abs(actual - expected) <= tolerance) }
            }
            for (sample in case["lines"]?.jsonArray.orEmpty().map { it.jsonObject }) {
                val l = frame.lines.getOrNull(sample.getValue("index").jsonPrimitive.int) ?: continue
                listOf(l.x1, l.y1, l.x2, l.y2, l.white, l.alpha, l.width).zip(sample.getValue("values").jsonArray.map { it.jsonPrimitive.double })
                    .forEach { (actual, expected) -> assertTrue("$key line: $actual != $expected", abs(actual - expected) <= tolerance) }
            }
        }
    }
}
