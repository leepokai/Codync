package com.codync.android

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ComfyStatusTest {
    private fun decode(text: String) = ComfyStatus.decode(Json.parseToJsonElement(text).jsonObject)

    @Test fun aRunningComfyWithAJobReadsAsTheCardDoes() {
        val status = decode("""{"id":"comfy","state":"running","error":null,"port":8188,
            "comfy":{"bundled":true,"notice":"No NVIDIA graphics card was found, so ComfyUI runs on the processor. Images take minutes instead of seconds."},
            "job":{"id":"p1","phase":"running","error":null,"files":[]}}""")
        assertTrue(status.active)
        assertEquals("Running", status.stateLabel)
        assertEquals("Making an image…", status.jobLabel)
        assertTrue(status.jobActive)
        assertTrue(status.notice!!.startsWith("No NVIDIA"))
    }

    @Test fun aStoppedComfyWithoutAJobOffersStartAndNoJobLine() {
        val status = decode("""{"id":"comfy","state":"stopped","error":null,"comfy":{"bundled":true,"notice":null},"job":null}""")
        assertFalse(status.active)
        assertEquals("Stopped", status.stateLabel)
        assertNull(status.jobLabel)
        assertNull(status.notice)
    }

    @Test fun aBuildWithoutComfyAndAnOlderHostAreToldApart() {
        assertFalse(decode("""{"state":"stopped","comfy":{"bundled":false}}""").bundled)
        // An older host without the card still reads as a stopped ComfyUI it may include.
        val bare = decode("""{}""")
        assertTrue(bare.bundled)
        assertEquals("stopped", bare.state)
    }

    @Test fun anErrorKeepsTheHostsReason() {
        val status = decode("""{"state":"error","error":"ComfyUI needs 10 GB free","comfy":{"bundled":true}}""")
        assertEquals("Stopped with an error", status.stateLabel)
        assertEquals("ComfyUI needs 10 GB free", status.error)
    }
}
