package com.codync.android

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class N8nStatusTest {
    private fun json(text: String) = Json.parseToJsonElement(text).jsonObject

    @Test fun aRunningN8nReadsAsTheDesktopCardDoes() {
        val status = N8nStatus.decode(json("""{"enabled":true,"state":"running","error":null,"port":5678,"path":"C:/n8n","mcp":true,"auth":null}"""))
        assertTrue(status.active && status.running && status.enabled)
        assertEquals("Running", status.stateLabel)
        assertEquals("C:/n8n", status.path)
        assertNull(status.error)
    }

    @Test fun aStartingOneIsActiveButNotYetRunning() {
        val status = N8nStatus.decode(json("""{"state":"starting"}"""))
        assertTrue(status.active)
        assertFalse(status.running)
        assertEquals("Starting {name}… the first start can take a few minutes.", status.stateLabel)
    }

    @Test fun aComputerWithoutAFolderStaysStoppedAndSaysSo() {
        val status = N8nStatus.decode(json("""{"enabled":false,"state":"stopped","path":null}"""))
        assertFalse(status.active)
        assertNull(status.path)
        assertEquals("Stopped", status.stateLabel)
        assertEquals("Stopped with an error", N8nStatus.decode(json("""{"state":"error","error":"port busy"}""")).stateLabel)
    }

    @Test fun anOlderHostsEmptyReplyReadsAsStopped() {
        val status = N8nStatus.decode(json("{}"))
        assertEquals("stopped", status.state)
        assertFalse(status.enabled)
    }

    @Test fun workflowsKeepTheirSwitchAndDropWhatHasNoId() {
        val list = N8nWorkflows.decode(json("""{"workflows":[
            {"id":"1","name":"Daily report","active":true,"updatedAt":"2026-11-01T10:00:00.000Z"},
            {"id":"2","name":"Draft","active":false},
            {"name":"No id","active":true}],"more":true}"""))
        assertEquals(listOf(N8nWorkflow("1", "Daily report", true), N8nWorkflow("2", "Draft", false)), list.workflows)
        assertTrue(list.more)
        assertTrue(N8nWorkflows.decode(json("{}")).workflows.isEmpty())
    }

    @Test fun runsReadTheirTimeAndNameTheirStatus() {
        val runs = decodeN8nRuns(json("""{"executions":[
            {"id":"1000","workflowId":"7","status":"success","mode":"trigger","startedAt":"2026-11-01T10:00:00.000Z","stoppedAt":"2026-11-01T10:00:02.000Z"},
            {"id":"1001","workflowId":"7","status":"running","startedAt":null},
            {"id":"1002","workflowId":null,"status":"crashed"},
            {"workflowId":"7","status":"success"}]}"""))
        assertEquals(3, runs.size)
        assertEquals(1_793_527_200_000L, runs[0].startedAtMillis)
        assertEquals("Succeeded", runs[0].statusLabel)
        assertNull(runs[1].startedAtMillis)
        assertEquals("Running", runs[1].statusLabel)
        assertNull(runs[2].workflowId)
        assertEquals("Crashed", runs[2].statusLabel)
        assertEquals("Waiting in the queue", N8nRun("1", null, "new", null).statusLabel)
        assertEquals("Unknown", N8nRun("1", null, "something-new", null).statusLabel)
    }
}
