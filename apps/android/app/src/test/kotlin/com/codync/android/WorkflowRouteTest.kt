package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Test

class WorkflowRouteTest {
    @Test fun aStepOpensAloneOnlyOutsideARun() {
        assertEquals(WorkflowRoute("w", stepId = "s"), workflowRoute("w", null, "s"))
        assertEquals(WorkflowRoute("w", runId = "r"), workflowRoute("w", "r", "s"))
        assertEquals(WorkflowRoute("w", stepId = "s"), workflowRoute("w", "", "s"))
        assertEquals(WorkflowRoute("w"), workflowRoute("w", " ", ""))
        assertEquals(WorkflowRoute(), workflowRoute(null, "r", "s"))
    }
}
