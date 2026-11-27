package com.codync.android

import com.codync.android.core.Graph
import com.codync.android.core.NodeKind
import com.codync.android.core.Point
import com.codync.android.core.WorkflowEdge
import com.codync.android.core.WorkflowNode
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Reading a graph as an outline. The shapes match `host/tests/fixtures/workflow-graph.json`
 * and `host/src/workflows/outline.rs`: a line, a condition branch, a loop, two conditions.
 */
class WorkflowOutlineTest {
    @Test fun `a straight line reads as one column`() {
        val graph = Graph(
            nodes = listOf(step("t", NodeKind.Trigger, 0), step("p", NodeKind.Prompt, 1), step("a", NodeKind.Agent, 2), step("o", NodeKind.Output, 3)),
            edges = listOf(edge("t", "a"), edge("p", "a"), edge("a", "o")),
        )
        val outline = requireNotNull(outlineOf(graph))
        assertEquals(listOf("p", "a", "o"), outline.main)
        assertNull(outline.condition)
        assertEquals(listOf("t", "p", "a", "o"), readingOrder(graph))
        assertEquals(mapOf("p" to 1, "a" to 2, "o" to 3), stepNumbers(graph))
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
    }

    @Test fun `two conditions and a loop are no outline`() {
        val two = Graph(
            nodes = listOf(step("t", NodeKind.Trigger, 0), step("c1", NodeKind.Condition, 1), step("c2", NodeKind.Condition, 2)),
            edges = emptyList(),
        )
        assertNull(outlineOf(two))
        val loop = Graph(
            nodes = listOf(step("t", NodeKind.Trigger, 0), step("a", NodeKind.Agent, 1)),
            edges = listOf(edge("t", "a"), edge("a", "a")),
        )
        assertNull(outlineOf(loop))
        assertTrue(loopEdges(loop).contains("a>a:text"))
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
