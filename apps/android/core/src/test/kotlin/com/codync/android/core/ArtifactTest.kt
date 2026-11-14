package com.codync.android.core

import java.security.MessageDigest
import java.time.LocalDate
import java.time.ZoneOffset
import java.util.Base64
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class ArtifactTest {
    private fun artifact(id: String = "p", updatedAt: Long = 10, title: String = "Weekly report", bytes: ByteArray = ByteArray(0)) =
        Artifact(id, "bot", title, bytes.size.toLong(), sha(bytes), 1, updatedAt, "card")

    private fun sha(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    private fun json(artifact: Artifact) = WireJson.encodeToJsonElement(Artifact.serializer(), artifact)

    @Test fun `an artifact card and a quote about an artifact read leniently`() {
        val card = Entry("e", 1, "bot", rev = 1, kind = "agent", data = buildJsonObject {
            put("text", "Artifact: Weekly report"); put("final", true)
            putJsonObject("artifact") { put("id", "p"); put("title", "Weekly report"); put("updatedAt", 10) }
        })
        assertEquals(ArtifactCard("p", "Weekly report", 10), card.artifact)
        assertNull(Entry("e", 1, "bot", rev = 1, kind = "agent", data = buildJsonObject { put("artifact", "p") }).artifact)
        val reply = Entry("r", 2, "bot", rev = 2, kind = "user", data = buildJsonObject {
            putJsonObject("replyTo") { put("id", "e"); put("author", "bot"); put("text", "Artifact: Weekly report"); put("artifact", "p") }
        })
        assertEquals("p", reply.replyTo?.artifact)
    }

    @Test fun `artifact events upsert details, and one this app can't read is passed over`() {
        val event = MirrorEvent.decode(buildJsonObject { put("type", "artifact"); put("artifact", json(artifact())) })
        assertEquals(MirrorEvent.ArtifactChanged(artifact()), event)
        val broken = buildJsonObject { put("type", "artifact"); putJsonObject("artifact") { put("id", "p") } }
        assertEquals(MirrorEvent.Ignored, MirrorEvent.decode(broken))
    }

    @Test fun `the cache keeps the newest details and reads again once a newer version is known`() {
        var cache = ArtifactCache().upsert(artifact(updatedAt = 10))
        assertTrue(cache.needsRead("p"))
        cache = cache.store(artifact(updatedAt = 10), "<h1>One</h1>")
        assertFalse(cache.needsRead("p"))
        // A card announcing a newer version, then the event bringing it.
        assertTrue(cache.needsRead("p", announced = 20))
        cache = cache.upsert(artifact(updatedAt = 20)).upsert(artifact(updatedAt = 10, title = "Stale"))
        assertEquals(20, cache.details.getValue("p").updatedAt)
        assertTrue(cache.needsRead("p"))
        // A rename keeps updatedAt and replaces the title.
        cache = cache.upsert(artifact(updatedAt = 20, title = "Renamed"))
        assertEquals("Renamed", cache.title(ArtifactCard("p", "Weekly report", 10)))
        assertEquals("Newer card", cache.title(ArtifactCard("p", "Newer card", 30)))
        cache = cache.fail("p", "offline")
        assertEquals("offline", cache.failures["p"])
        assertNull(cache.store(artifact(updatedAt = 20), "<h1>Two</h1>").failures["p"])
    }

    @Test fun `the Library holds the artifacts known here and the listed files, and skips what it can't read`() {
        val listed = buildJsonObject {
            putJsonArray("items") {
                addJsonObject { put("kind", "artifact"); put("at", 5); put("artifact", json(artifact(updatedAt = 5, title = "Old"))) }
                addJsonObject { put("kind", "file"); put("at", 30); put("entryId", "e1")
                    putJsonObject("file") { put("id", "f"); put("name", "report.pdf"); put("size", 3); put("sha256", "a".repeat(64)) } }
                addJsonObject { put("kind", "attachment"); put("at", 20); put("entryId", "e2")
                    putJsonObject("file") { put("id", "u"); put("name", "photo.png"); put("size", 4) } }
                addJsonObject { put("kind", "video"); put("at", 40) }
                addJsonObject { put("kind", "file"); put("at", 50) }
            }
        }.getValue("items").jsonArray.mapNotNull { LibraryItem.decode(it.jsonObject) }
        assertEquals(3, listed.size)
        // The artifact as an event left it, not as listed.
        val items = ArtifactCache().upsert(artifact(updatedAt = 25, title = "New")).upsert(artifact("x", 99).copy(botId = "other")).library("bot", listed)
        assertEquals(listOf(30L, 25L, 20L), items.map { it.at })
        assertEquals("New", (items[1] as LibraryItem.ArtifactItem).artifact.title)
        assertEquals(SharedFile("f", "report.pdf", 3, "a".repeat(64)), (items[0] as LibraryItem.FileItem).file)
        assertEquals("e2", (items[2] as LibraryItem.AttachmentItem).entryId)
    }

    @Test fun `the Library groups items by the day they were published or shared`() {
        val today = LocalDate.of(2026, 10, 24)
        fun at(day: LocalDate, hour: Int) = day.atTime(hour, 0).toInstant(ZoneOffset.UTC).toEpochMilli()
        val items = listOf(at(today, 9), at(today.minusDays(1), 23), at(today.minusDays(6), 0), at(today.minusDays(7), 23))
            .map { LibraryItem.ArtifactItem(it, artifact(updatedAt = it)) }
        val sections = LibraryPeriod.sections(items, today, ZoneOffset.UTC)
        assertEquals(listOf(LibraryPeriod.Today, LibraryPeriod.Yesterday, LibraryPeriod.ThisWeek, LibraryPeriod.Earlier), sections.map { it.first })
        assertTrue(sections.all { it.second.size == 1 })
    }

    /** Serves the versions in turn as readArtifact does: one chunk per call, the version moving on after `switchAfter` calls. */
    private fun host(vararg versions: Pair<Artifact, ByteArray>, switchAfter: Int = Int.MAX_VALUE, corrupt: Boolean = false): Pair<HostFiles, () -> Int> {
        var calls = 0
        val files = HostFiles { method, body, _ ->
            assertEquals("readArtifact", method)
            assertEquals("p", body.getValue("artifactId").jsonPrimitive.content)
            val (artifact, bytes) = versions[minOf(calls++ / switchAfter, versions.size - 1)]
            val offset = body.getValue("offset").jsonPrimitive.long.toInt()
            val end = minOf(offset + HostFiles.CHUNK_BYTES, bytes.size)
            val data = if (corrupt) bytes.copyOfRange(offset, end).reversedArray() else bytes.copyOfRange(offset, end)
            buildJsonObject { put("data", Base64.getEncoder().encodeToString(data)); put("size", bytes.size); put("artifact", json(artifact)) }
        }
        return files to { calls }
    }

    @Test fun `messages about an artifact quote its card, from a draft of their own, on a host with the Library`() {
        assertEquals(ReplyQuote("e", "bot", "Artifact: Weekly report", "p"), artifactQuote("e", "bot", "p", "Weekly report"))
        assertEquals("bot/artifact/p", artifactDraftKey("bot", "p"))
        assertTrue(hasLibrary("2.25.0"))
        assertFalse(hasLibrary("2.24.3"))
        assertFalse(hasLibrary(null))
    }

    @Test fun `an artifact is read in chunks and checked`() = runBlocking {
        val html = "<h1>Weekly report</h1>".repeat(40_000).toByteArray()
        val (files, calls) = host(artifact(bytes = html) to html)
        val (read, text) = files.readArtifact("bot", "p")
        assertEquals(artifact(bytes = html), read)
        assertEquals(String(html), text)
        assertEquals(3, calls())
        val empty = ByteArray(0)
        assertEquals("", host(artifact(bytes = empty) to empty).first.readArtifact("bot", "p").second)
    }

    @Test fun `an artifact published again while it is read is read again`() = runBlocking {
        val one = "a".repeat(HostFiles.CHUNK_BYTES + 1).toByteArray()
        val two = "b".repeat(HostFiles.CHUNK_BYTES + 5).toByteArray()
        val (files, _) = host(artifact(bytes = one) to one, artifact(updatedAt = 20, bytes = two) to two, switchAfter = 1)
        val (read, text) = files.readArtifact("bot", "p")
        assertEquals(20, read.updatedAt)
        assertEquals(String(two), text)
    }

    @Test fun `an artifact whose HTML never matches its hash or whose chunk is invalid fails`() {
        val html = "<p>Updated</p>".toByteArray()
        assertFails<java.io.IOException> { runBlocking { host(artifact(bytes = html) to html, corrupt = true).first.readArtifact("bot", "p") } }
        val bad = HostFiles { _, _, _ -> buildJsonObject { put("data", "%%%"); put("size", 3); put("artifact", json(artifact().copy(size = 3))) } }
        assertFails<IllegalArgumentException> { runBlocking { bad.readArtifact("bot", "p") } }
    }
}
