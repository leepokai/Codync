package com.codync.android.core

import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class WorkflowTest {
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

    @Test fun `an unknown step kind decodes and the detail stays read only`() {
        val node = WireJson.decodeFromString<WorkflowNode>("""{"id":"n","kind":"future","at":{"x":1,"y":2},"fields":{}}""")
        assertEquals(NodeKind.Unknown, node.kind)
        val detail = WorkflowDetail(id = "w", editMode = "outline", graph = Graph(nodes = listOf(node)))
        assertEquals(EditMode.ReadOnly, detail.resolvedMode())
        val known = WireJson.decodeFromString<WorkflowNode>("""{"id":"t","kind":"trigger","at":{"x":80,"y":80},"fields":{"trigger":"manual"}}""")
        assertEquals(NodeKind.Trigger, known.kind)
        assertEquals("Manual", known.stepTitle())
    }

    @Test fun `ports follow the host and a summary puts an open run first`() {
        assertEquals(listOf("text"), ports(NodeKind.Trigger).second)
        assertEquals(0, ports(NodeKind.Trigger).first.size)
        assertEquals(0, ports(NodeKind.Prompt).first.size)
        assertEquals(listOf("text"), ports(NodeKind.Agent).first)
        assertEquals(listOf("text"), ports(NodeKind.Agent).second)
        assertEquals(listOf("text"), ports(NodeKind.Condition).second)
        assertEquals(0, ports(NodeKind.Output).second.size)
        assertEquals(0, ports(NodeKind.Note).first.size)
        assertEquals(0, ports(NodeKind.Note).second.size)

        val quiet = WorkflowSummary("old", updatedAt = 50, lastRun = WorkflowLastRun(1, "done"))
        val busy = WorkflowSummary("new", updatedAt = 10, running = "approval")
        assertEquals(listOf("new", "old"), listOf(quiet, busy).listed().map { it.id })
        assertEquals("Needs you", busy.statusLabel())
        assertEquals("Done", quiet.statusLabel())
        assertEquals("Running", quiet.copy(running = "stopping").statusLabel())
        assertNull(WorkflowSummary("empty").statusLabel())
    }

    @Test fun `a graph round trips strokes edges and a condition branch`() {
        val graph = Graph(
            nodes = listOf(node("t", NodeKind.Trigger), node("a", NodeKind.Agent)),
            edges = listOf(WorkflowEdge("t>a:text", "t", "a", "text", "yes")),
            strokes = listOf(WorkflowStroke("s", "pen", "#1c1c1e", 2.5, listOf(Point(10.0, 12.0)))),
        )
        val again = WireJson.decodeFromString<Graph>(WireJson.encodeToString(graph))
        assertEquals(graph.strokes, again.strokes)
        assertEquals("yes", again.edges.single().branch)
        assertEquals("text", again.edges.single().type)
        assertEquals("Assistant", graph.nodes[1].stepTitle())
        assertEquals("Choose a bot", graph.nodes[1].stepSummary())
    }

    private fun node(id: String, kind: NodeKind) = WorkflowNode(
        id, kind, Point(0.0, 0.0),
        fields = when (kind) {
            NodeKind.Trigger -> WireJson.parseToJsonElement("""{"trigger":"manual"}""").let { it as kotlinx.serialization.json.JsonObject }
            else -> kotlinx.serialization.json.JsonObject(emptyMap())
        },
    )
}
