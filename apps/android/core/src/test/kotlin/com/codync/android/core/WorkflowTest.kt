package com.codync.android.core

import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class WorkflowTest {
    @Test fun `search finds workflows by title or id prefix and keeps them all for a blank query`() {
        val list = listOf(WorkflowSummary("a1b2", "Nightly build"), WorkflowSummary("c3d4", "Release notes"))
        assertEquals(list, workflowMatches(list, " "))
        assertEquals(listOf("a1b2"), workflowMatches(list, "BUILD").map { it.id })
        assertEquals(listOf("c3d4"), workflowMatches(list, " c3 ").map { it.id })
        assertEquals(emptyList<WorkflowSummary>(), workflowMatches(list, "b2"))
    }

    @Test fun `hello workflows defaults to false and a workflow event that does not parse is ignored`() {
        val plain = MirrorEvent.decode(buildJsonObject { put("type", "hello"); put("hostId", "h"); put("rev", 1) }) as MirrorEvent.Hello
        assertFalse(plain.workflows)
        val offered = MirrorEvent.decode(buildJsonObject {
            put("type", "hello"); put("hostId", "h"); put("rev", 1); put("workflows", true)
        }) as MirrorEvent.Hello
        assertTrue(offered.workflows)
        val off = MirrorEvent.decode(buildJsonObject {
            put("type", "hello"); put("hostId", "h"); put("rev", 1); put("workflows", false)
        }) as MirrorEvent.Hello
        assertFalse(off.workflows)

        assertEquals(MirrorEvent.Ignored, MirrorEvent.decode(buildJsonObject { put("type", "workflow") }))
        assertEquals(MirrorEvent.Ignored, MirrorEvent.decode(buildJsonObject { put("type", "workflow"); put("change", "graph") }))
        assertEquals(MirrorEvent.Ignored, MirrorEvent.decode(buildJsonObject {
            put("type", "workflow"); put("change", "later"); put("workflowId", "w")
        }))
        assertEquals(MirrorEvent.Ignored, MirrorEvent.decode(buildJsonObject {
            put("type", "workflow"); put("change", "graph"); put("workflowId", "w"); put("runId", buildJsonObject { put("nope", true) })
        }))
        val step = MirrorEvent.decode(buildJsonObject {
            put("type", "workflow"); put("change", "step"); put("workflowId", "w"); put("runId", JsonNull); put("stepId", "s")
        }) as MirrorEvent.Workflow
        assertEquals("step", step.change.change)
        assertEquals("w", step.change.workflowId)
        assertNull(step.change.runId)
        assertEquals("s", step.change.stepId)
    }

    @Test fun `an unknown step kind decodes`() {
        val node = WireJson.decodeFromString<WorkflowNode>("""{"id":"n","kind":"future","at":{"x":1,"y":2},"fields":{}}""")
        assertEquals(NodeKind.Unknown, node.kind)
        val known = WireJson.decodeFromString<WorkflowNode>("""{"id":"t","kind":"trigger","at":{"x":80,"y":80},"fields":{"trigger":"manual"}}""")
        assertEquals(NodeKind.Trigger, known.kind)
        assertEquals("Manual", known.stepTitle())
    }

    @Test fun `a summary puts an open run first`() {
        val quiet = WorkflowSummary("old", updatedAt = 50, lastRun = WorkflowLastRun(1, "done"))
        val busy = WorkflowSummary("new", updatedAt = 10, running = "approval")
        assertEquals(listOf("new", "old"), listOf(quiet, busy).listed().map { it.id })
        assertEquals("Needs you", busy.statusLabel())
        assertEquals("Done", quiet.statusLabel())
        assertEquals("Running", quiet.copy(running = "stopping").statusLabel())
        assertNull(WorkflowSummary("empty").statusLabel())
    }

    @Test fun `a graph decodes edges with a condition branch and names its steps`() {
        val graph = Graph(
            nodes = listOf(node("t", NodeKind.Trigger), node("a", NodeKind.Agent)),
            edges = listOf(WorkflowEdge("t>a:text", "t", "a", "text", "yes")),
        )
        val again = WireJson.decodeFromString<Graph>(WireJson.encodeToString(graph))
        assertEquals("yes", again.edges.single().branch)
        assertEquals("text", again.edges.single().type)
        assertEquals("Agent", graph.nodes[1].stepTitle())
        assertEquals("Choose a bot", graph.nodes[1].stepSummary())
        val named = graph.nodes[1].copy(fields = buildJsonObject { put("botId", "b"); put("task", "") })
        assertEquals("王林", named.stepWho { _ -> "王林" })
        assertEquals("Do this step with the input.", named.stepSummary({ _: String -> "王林" }))
        val prompt = node("p", NodeKind.Prompt).copy(fields = buildJsonObject { put("text", "你们是什么模型？") })
        assertEquals("Prompt", prompt.stepTitle())
        assertEquals("你们是什么模型？", prompt.stepSummary())
    }

    @Test fun `parents follow incoming edges once`() {
        val graph = Graph(
            nodes = listOf(node("p", NodeKind.Prompt), node("a", NodeKind.Agent), node("b", NodeKind.Agent), node("o", NodeKind.Output)),
            edges = listOf(
                WorkflowEdge("p>a", "p", "a"),
                WorkflowEdge("p>b", "p", "b"),
                WorkflowEdge("a>o", "a", "o"),
                WorkflowEdge("b>o", "b", "o"),
                WorkflowEdge("a>o-again", "a", "o"),
            ),
        )
        assertEquals(emptyList<String>(), graph.parentsOf("p"))
        assertEquals(listOf("p"), graph.parentsOf("a"))
        assertEquals(listOf("a", "b"), graph.parentsOf("o"))
    }

    private fun node(id: String, kind: NodeKind) = WorkflowNode(
        id, kind, Point(0.0, 0.0),
        fields = when (kind) {
            NodeKind.Trigger -> WireJson.parseToJsonElement("""{"trigger":"manual"}""").let { it as kotlinx.serialization.json.JsonObject }
            else -> kotlinx.serialization.json.JsonObject(emptyMap())
        },
    )
}
