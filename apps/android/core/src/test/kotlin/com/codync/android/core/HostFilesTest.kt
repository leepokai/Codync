package com.codync.android.core

import java.nio.file.Files
import java.util.Base64
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class HostFilesTest {
    @Test fun `uploads stream bounded ordered chunks including an empty file`() = runBlocking {
        val root = Files.createTempDirectory("codync-files").toFile()
        try {
            for (size in listOf(0, HostFiles.CHUNK_BYTES + 13)) {
                val source = root.resolve("source").apply { writeBytes(ByteArray(size) { (it % 255).toByte() }) }
                var offset = 0L
                var calls = 0
                val files = HostFiles { method, body, timeout ->
                    assertEquals("upload", method); assertEquals(60_000, timeout)
                    assertEquals(offset, body.getValue("offset").jsonPrimitive.long)
                    val bytes = Base64.getDecoder().decode(body.getValue("data").jsonPrimitive.content)
                    assertTrue(bytes.size <= HostFiles.CHUNK_BYTES)
                    for (i in bytes.indices) assertEquals(((offset + i) % 255).toByte(), bytes[i])
                    offset += bytes.size; calls++
                    assertEquals(offset == size.toLong(), body.getValue("done").jsonPrimitive.boolean)
                    JsonObject(emptyMap())
                }
                files.upload("bot", "upload", OutgoingFile(source.path, "source", size.toLong()))
                assertEquals(size.toLong(), offset)
                assertEquals(if (size == 0) 1 else 2, calls)
            }
        } finally { root.deleteRecursively() }
    }

    @Test fun `truncated downloads cannot publish a partial cache file`() = runBlocking {
        val root = Files.createTempDirectory("codync-files").toFile()
        try {
            var calls = 0
            val files = HostFiles { _, _, _ ->
                calls++
                buildJsonObject { put("size", 4); put("data", if (calls == 1) "YWI=" else "") }
            }
            val destination = root.resolve("saved")
            assertFails<IllegalArgumentException> { files.download("bot", Attachment("id", "file", 4), destination) }
            assertFalse(destination.exists())
            assertFalse(root.resolve("saved.partial").exists())
        } finally { root.deleteRecursively() }
    }

    private fun shared(bytes: ByteArray, name: String = "report.pdf", sha256: String? = null) = SharedFile("file", name, bytes.size.toLong(),
        sha256 ?: java.security.MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) })

    /** Serves `bytes` as the host's readFile does, in chunks from the requested offset. */
    private fun host(bytes: ByteArray) = HostFiles { method, body, _ ->
        assertEquals("readFile", method)
        assertEquals("entry", body.getValue("entryId").jsonPrimitive.content)
        val offset = body.getValue("offset").jsonPrimitive.long.toInt()
        val end = minOf(offset + HostFiles.CHUNK_BYTES, bytes.size)
        buildJsonObject { put("size", bytes.size); put("data", Base64.getEncoder().encodeToString(bytes.copyOfRange(offset, end))) }
    }

    @Test fun `a shared file arrives whole and verified in its own folder`() = runBlocking {
        val root = Files.createTempDirectory("codync-shared").toFile()
        try {
            for (size in listOf(0, HostFiles.CHUNK_BYTES * 2 + 5)) {
                val bytes = ByteArray(size) { (it % 251).toByte() }
                val progress = mutableListOf<Long>()
                val folder = root.resolve("download-$size")
                val saved = host(bytes).downloadShared("entry", shared(bytes), folder) { progress += it }
                assertEquals(folder.resolve("ready/report.pdf"), saved)
                assertArrayEquals(bytes, saved.readBytes())
                assertEquals(size.toLong(), progress.last())
                assertFalse(folder.resolve(".download-partial").exists())
            }
        } finally { root.deleteRecursively() }
    }

    @Test fun `a changed or truncated shared file leaves nothing behind`() = runBlocking {
        val root = Files.createTempDirectory("codync-shared").toFile()
        try {
            val bytes = ByteArray(10) { it.toByte() }
            val folder = root.resolve("download")
            val integrity = assertFails<java.io.IOException> { host(bytes).downloadShared("entry", shared(bytes, sha256 = "0".repeat(64)), folder) }
            assertEquals("The file integrity check failed. Retry the download.", integrity.message)
            assertFalse(folder.exists())
            val short = HostFiles { _, _, _ -> buildJsonObject { put("size", 10); put("data", "YWI=") } }
            assertFails<java.io.IOException> { short.downloadShared("entry", shared(bytes), folder) }
            assertFalse(folder.exists())
        } finally { root.deleteRecursively() }
    }

    @Test fun `shared file names and hashes are checked before anything is read`() = runBlocking {
        val files = HostFiles { _, _, _ -> error("read before validation") }
        for (file in listOf(shared(ByteArray(1), name = "../escape"), shared(ByteArray(1), name = ".."), shared(ByteArray(1), name = "a\u0000b"),
            shared(ByteArray(1), sha256 = "ABC"), SharedFile("file", "big", MAX_FILE_BYTES + 1, "0".repeat(64)))) {
            val error = assertFails<IllegalArgumentException> { files.downloadShared("entry", file, Files.createTempDirectory("codync-shared").toFile().resolve("x")) }
            assertEquals("Invalid file metadata.", error.message)
        }
        shared(ByteArray(1), name = ".download-partial").validate()
    }
}
