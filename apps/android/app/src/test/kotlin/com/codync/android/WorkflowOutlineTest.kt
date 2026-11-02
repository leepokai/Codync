package com.codync.android

import com.codync.android.core.EditMode
import com.codync.android.core.Graph
import com.codync.android.core.NodeKind
import com.codync.android.core.Point
import com.codync.android.core.WorkflowEdge
import com.codync.android.core.WorkflowNode
import com.codync.android.core.WorkflowStroke
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Outline round trip. The shapes match `host/tests/fixtures/workflow-graph.json`
 * and `host/src/workflows/outline.rs`: a line, a condition branch, a loop, two conditions.
 */
class WorkflowOutlineTest {
    @Test fun `a straight line round trips and lays out in columns`() {
        val graph = Graph(
            nodes = listOf(step("t", NodeKind.Trigger, 0), step("p", NodeKind.Prompt, 1), step("a", NodeKind.Agent, 2), step("o", NodeKind.Output, 3)),
            edges = listOf(edge("t", "a"), edge("p", "a"), edge("a", "o")),
            strokes = listOf(WorkflowStroke("ink", "pen", "#1c1c1e", 2.5, listOf(Point(10.0, 12.0)))),
        )
        val outline = requireNotNull(outlineOf(graph))
        assertEquals(listOf("p", "a", "o"), outline.main)
        assertNull(outline.condition)
        val again = graphFromOutline(graph, outline)
        assertEquals(outline.main, outlineOf(again)?.main)
        assertEquals(graph.strokes, again.strokes)
        assertEquals("t>a:text", again.edges.first { it.from == "t" }.id)
        assertEquals("p>a:text", again.edges.first { it.from == "p" }.id)
        assertEquals(80.0, again.nodes.first { it.id == "t" }.at.x, 0.0)
        assertEquals(480.0, again.nodes.first { it.id == "p" }.at.x, 0.0)
        assertEquals(80.0, again.nodes.first { it.id == "p" }.at.y, 0.0)
        assertEquals(EditMode.Outline, editMode(graph))
        assertEquals(listOf("t", "p", "a", "o"), readingOrder(again))
    }

    @Test fun `a condition branch round trips yes no and the join`() {
        val graph = Graph(
            nodes = listOf(
                step("t", NodeKind.Trigger, 0),
                step("c", NodeKind.Condition, 1),
                step("a", NodeKind.Agent, 2, 80.0),
                step("b", NodeKind.Agent, 2, 300.0),
                step("o", NodeKind.Output, 3),
            ),
            edges = listOf(edge("t", "c"), edge("c", "a", "yes"), edge("c", "b", "no"), edge("a", "o"), edge("b", "o")),
        )
        val outline = requireNotNull(outlineOf(graph))
        assertEquals(emptyList<String>(), outline.main)
        assertEquals("c", outline.condition)
        assertEquals(listOf("a"), outline.yes)
        assertEquals(listOf("b"), outline.no)
        assertEquals(listOf("o"), outline.after)
        val again = graphFromOutline(graph, outline)
        val back = requireNotNull(outlineOf(again))
        assertEquals(outline.yes, back.yes)
        assertEquals(outline.no, back.no)
        assertEquals(outline.after, back.after)
        assertTrue(again.edges.any { it.id == "c.yes>a:text" })
        assertTrue(again.edges.any { it.id == "c.no>b:text" })
        assertEquals(300.0, again.nodes.first { it.id == "b" }.at.y, 0.0)
        assertTrue(unfedPrompts(outline, graph.nodes.associate { it.id to it.kind }).isEmpty())
    }

    @Test fun `a prompt that ends a column does not feed a step`() {
        val kinds = mapOf("a" to NodeKind.Agent, "p" to NodeKind.Prompt, "c" to NodeKind.Condition)
        assertEquals(setOf("p"), unfedPrompts(Outline("t", main = listOf("a", "p")), kinds))
        assertTrue(unfedPrompts(Outline("t", main = listOf("p"), condition = "c"), kinds).isEmpty())
        assertEquals(setOf("p"), unfedPrompts(Outline("t", condition = "c", yes = listOf("p")), kinds))
    }

    @Test fun `two conditions and a loop are fields only`() {
        val two = Graph(
            nodes = listOf(step("t", NodeKind.Trigger, 0), step("c1", NodeKind.Condition, 1), step("c2", NodeKind.Condition, 2)),
            edges = emptyList(),
        )
        assertNull(outlineOf(two))
        assertEquals(EditMode.Fields, editMode(two))
        val loop = Graph(
            nodes = listOf(step("t", NodeKind.Trigger, 0), step("a", NodeKind.Agent, 1)),
            edges = listOf(edge("t", "a"), edge("a", "a")),
        )
        assertNull(outlineOf(loop))
        assertEquals(EditMode.Fields, editMode(loop))
        assertTrue(loopEdges(loop).contains("a>a:text"))
    }

    @Test fun `moving and rebuilding keeps the same columns`() {
        val graph = Graph(
            nodes = listOf(step("t", NodeKind.Trigger, 0), step("a", NodeKind.Agent, 1), step("b", NodeKind.Approval, 2)),
            edges = listOf(edge("t", "a"), edge("a", "b")),
        )
        val outline = requireNotNull(outlineOf(graph)).moved("b", down = false)
        val again = graphFromOutline(graph, requireNotNull(outline))
        assertEquals(listOf("b", "a"), outlineOf(again)?.main)
    }

    @Test fun `changing a source moves the step and keeps an outline`() {
        val graph = Graph(
            nodes = listOf(step("t", NodeKind.Trigger, 0), step("a", NodeKind.Agent, 1), step("b", NodeKind.Agent, 2), step("o", NodeKind.Output, 3)),
            edges = listOf(edge("t", "a"), edge("a", "b"), edge("b", "o")),
        )
        val outline = requireNotNull(outlineOf(graph))
        val kinds = graph.nodes.associate { it.id to it.kind }
        assertEquals(SourceTarget.Node("t"), outline.sourceTarget("a", kinds))
        assertEquals(SourceTarget.Node("b"), outline.sourceTarget("o", kinds))
        val moved = requireNotNull(graph.withSource("o", "t"))
        assertEquals(listOf("t"), moved.parentsOf("o"))
        assertEquals(listOf("t"), moved.parentsOf("a"))
        assertEquals(listOf("o", "a", "b"), outlineOf(moved)?.main)
    }

    private fun step(id: String, kind: NodeKind, column: Int, y: Double = 80.0) = WorkflowNode(
        id, kind, Point(80.0 + column * 400.0, y),
        fields = when (kind) {
            NodeKind.Trigger -> buildJsonObject { put("trigger", "manual") }
            NodeKind.Output -> buildJsonObject { put("target", "notify") }
            NodeKind.Condition -> buildJsonObject { put("op", "empty") }
            NodeKind.Prompt -> buildJsonObject { put("text", "Remember this") }
            else -> JsonObject(emptyMap())
        },
    )

    private fun edge(from: String, to: String, branch: String? = null) =
        WorkflowEdge(if (branch == null) "$from>$to:text" else "$from.$branch>$to:text", from, to, "text", branch)
}
