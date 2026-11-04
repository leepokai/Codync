package com.codync.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class RoutineFlowTest {
    private fun link(from: String, to: String) = FlowLink("$from-$to", "r", from, to, "", 1)
    private fun step(bot: String, status: String, at: Long = 1) = FlowStep("s-$bot", "run", bot, status = status, updatedAt = at)

    private fun canvas(links: List<FlowLink>, steps: List<FlowStep> = emptyList(), positions: List<FlowPosition> = emptyList(), placed: List<String> = emptyList()): FlowCanvas {
        val routine = Routine("r", "a", "Digest", "", emptyList(), true)
        return FlowCanvas("a", 1, listOf(FlowRoutineCard(routine, steps = steps)), links, positions, placed)
    }

    @Test fun `depth is the longest path from the routine bot and a bot not reached is absent`() {
        val d = RoutineFlow.depths("a", listOf(link("a", "b"), link("b", "c"), link("a", "c"), link("x", "y")))
        assertEquals(listOf(0, 1, 2), listOf(d["a"], d["b"], d["c"]))
        assertFalse("x" in d)
    }

    @Test fun `the connect rules are the hosts`() {
        val c = canvas(listOf(link("a", "b"), link("b", "c")))
        fun problem(from: String, to: String) = RoutineFlow.connectProblem(c, "r", from, to)
        assertTrue(problem("b", "b")!!.contains("itself"))
        assertTrue(problem("c", "a")!!.contains("own bot starts"))
        assertTrue(problem("x", "d")!!.contains("doesn't reach"))
        assertTrue(problem("a", "b")!!.contains("already connected"))
        assertTrue(problem("c", "b")!!.contains("loop"))
        assertNull(problem("b", "d"))
        assertNull("a second path to a bot is a join", problem("a", "c"))
        assertTrue(RoutineFlow.connectProblem(c, "nope", "a", "b")!!.contains("not on this canvas"))
    }

    @Test fun `the depth limit counts what already hangs below the bot being moved`() {
        val c = canvas(listOf(link("a", "b"), link("a", "x"), link("x", "y"), link("y", "z")))
        assertTrue(RoutineFlow.connectProblem(c, "r", "b", "x", 3)!!.contains("at most 3"))
        assertNull(RoutineFlow.connectProblem(c, "r", "b", "x", 4))
    }

    @Test fun `nodes never moved sit in columns by depth and a saved position wins`() {
        val c = canvas(listOf(link("a", "b"), link("b", "c")), positions = listOf(FlowPosition("bot:b", 5.0, 7.0)), placed = listOf("z"))
        val nodes = RoutineFlow.layout(c).associateBy { it.key }
        assertTrue(nodes.getValue("routine:r").x < nodes.getValue("bot:a").x)
        assertTrue(nodes.getValue("bot:a").x < nodes.getValue("bot:c").x)
        assertEquals(Triple(5.0, 7.0, true), nodes.getValue("bot:b").let { Triple(it.x, it.y, it.saved) })
        assertTrue("a bot no flow reaches stands after the flows", nodes.getValue("bot:z").x > nodes.getValue("bot:c").x)
        val fan = RoutineFlow.layout(canvas(listOf(link("a", "b"), link("a", "c")))).associateBy { it.key }
        assertTrue(fan.getValue("bot:b").x == fan.getValue("bot:c").x && fan.getValue("bot:b").y != fan.getValue("bot:c").y)
    }

    @Test fun `edges are each routine to its bot then one per link`() {
        val edges = RoutineFlow.edges(canvas(listOf(link("a", "b"))))
        assertEquals(listOf(Triple("routine:r", "bot:a", null), Triple("bot:a", "bot:b", "a-b")), edges.map { Triple(it.from, it.to, it.linkId) })
    }

    @Test fun `the list mirrors the canvas depth first with the step of each bot`() {
        val c = canvas(listOf(link("a", "b"), link("b", "c"), link("a", "d")), listOf(step("a", "succeeded"), step("b", "failed"), step("c", "skipped")))
        assertEquals(listOf(Triple("a", 0, "succeeded"), Triple("b", 1, "failed"), Triple("c", 2, "skipped"), Triple("d", 1, null)),
            RoutineFlow.rows(c).map { Triple(it.botId, it.depth, it.step?.status) })
    }

    @Test fun `a loop in the links cannot run the list away`() {
        assertEquals(listOf("a", "b"), RoutineFlow.rows(canvas(listOf(link("a", "b"), link("b", "a")))).map { it.botId })
    }

    @Test fun `zoom keeps the point under the fingers still and stays in bounds`() {
        val view = RoutineFlow.zoomAt(RoutineFlow.View(10f, 20f, 1f), 2f, 100f, 100f)
        assertEquals(Triple(2f, -80f, -60f), Triple(view.scale, view.x, view.y))
        assertEquals(2f, RoutineFlow.zoomAt(RoutineFlow.View(), 100f, 0f, 0f).scale, 0f)
        assertEquals(0.3f, RoutineFlow.zoomAt(RoutineFlow.View(), 0.001f, 0f, 0f).scale, 0f)
        assertTrue(RoutineFlow.pinchFactor(100f, 200f) > 1f && RoutineFlow.pinchFactor(200f, 100f) < 1f)
    }

    @Test fun `fit shows every node`() {
        val nodes = RoutineFlow.layout(canvas(listOf(link("a", "b"))))
        val view = RoutineFlow.fit(nodes, 1000f, 600f)
        for (n in nodes) {
            assertTrue(n.x * view.scale + view.x >= 0 && (n.x + RoutineFlow.NODE_WIDTH) * view.scale + view.x <= 1000)
        }
        assertEquals(RoutineFlow.View(), RoutineFlow.fit(emptyList(), 1000f, 600f))
    }

    @Test fun `a drop finds the card under it`() {
        val nodes = RoutineFlow.layout(canvas(listOf(link("a", "b"))))
        val a = nodes.first { it.key == "bot:a" }
        assertEquals("bot:a", RoutineFlow.nodeAt(nodes, a.x + 5, a.y + 5)?.key)
        assertNull(RoutineFlow.nodeAt(nodes, -999.0, -999.0))
    }

    @Test fun `the canvas decodes the hosts answer and the event decodes`() {
        val json = """{"botId":"a","rev":3,"routines":[{"routine":{"id":"r","botId":"a","name":"D","instruction":"i","triggers":[],"enabled":true},
            "recentRuns":[],"upcomingRuns":[1],"steps":[{"id":"s","runId":"x","botId":"a","deliveryId":null,"input":"","output":null,"status":"running","error":null,"createdAt":1,"updatedAt":2}]}],
            "links":[{"id":"l","routineId":"r","fromBotId":"a","toBotId":"b","instruction":"go","createdAt":1}],"positions":[{"nodeKey":"bot:a","x":1.5,"y":2}],"placedBotIds":["b"],"extra":1}"""
        val decoded = WireJson.decodeFromString<FlowCanvas>(json)
        assertEquals(listOf("running"), decoded.routines.single().steps.map { it.status })
        assertEquals("Running", decoded.routines.single().steps.single().label)
        val event = MirrorEvent.decode(WireJson.parseToJsonElement("""{"type":"routineFlow","rev":9,"botIds":["a","b"]}""").jsonObject)
        assertEquals(MirrorEvent.RoutineFlow(9, listOf("a", "b")), event)
        assertEquals(MirrorEvent.Ignored, MirrorEvent.decode(WireJson.parseToJsonElement("""{"type":"routineFlow"}""").jsonObject))
    }
}
