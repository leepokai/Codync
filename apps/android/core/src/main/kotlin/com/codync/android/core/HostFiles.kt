package com.codync.android.core

import java.io.File
import java.io.IOException
import java.security.MessageDigest
import java.util.Base64
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.*

/** The existing host file protocol, with memory bounded to one chunk. */
class HostFiles(private val call: suspend (String, JsonObject, Long) -> JsonElement) {
    constructor(channel: EncryptedChannel) : this({ method, body, timeout -> channel.call(method, body, timeout) })

    suspend fun upload(botId: String, id: String, file: OutgoingFile, progress: (Long) -> Unit = {}) = withContext(Dispatchers.IO) {
        val source = File(file.path)
        require(file.size in 0..MAX_FILE_BYTES && source.length() == file.size && source.isFile) { "The selected file is no longer available. Choose it again." }
        source.inputStream().use { input ->
            var offset = 0L
            do {
                val buffer = ByteArray(minOf(CHUNK_BYTES.toLong(), file.size - offset).toInt())
                var count = 0
                while (count < buffer.size) {
                    val read = input.read(buffer, count, buffer.size - count)
                    if (read < 0) break
                    count += read
                }
                val chunk = if (count == buffer.size) buffer else buffer.copyOf(count)
                require(offset + chunk.size <= file.size) { "The selected file changed during upload." }
                val done = offset + chunk.size == file.size
                require(done || chunk.isNotEmpty()) { "The selected file changed during upload." }
                call("upload", buildJsonObject {
                    put("botId", botId); put("uploadId", id); put("name", file.name)
                    put("offset", offset); put("data", Base64.getEncoder().encodeToString(chunk)); put("done", done)
                }, 60_000)
                offset += chunk.size
                progress(offset)
            } while (offset < file.size)
            require(input.read() == -1) { "The selected file changed during upload." }
        }
    }

    suspend fun download(botId: String, file: Attachment, destination: File) = withContext(Dispatchers.IO) {
        require(file.size in 0..MAX_FILE_BYTES)
        val partial = File(destination.parentFile, destination.name + ".partial")
        try {
            partial.outputStream().use { output ->
                var offset = 0L
                do {
                    val result = call("readUpload", buildJsonObject { put("botId", botId); put("uploadId", file.id); put("offset", offset) }, 60_000).jsonObject
                    val size = result.getValue("size").jsonPrimitive.long
                    require(size == file.size) { "The computer returned a different file size." }
                    val bytes = Base64.getDecoder().decode(result.getValue("data").jsonPrimitive.content)
                    require(bytes.size <= CHUNK_BYTES && offset + bytes.size <= size && (bytes.isNotEmpty() || size == 0L)) {
                        "The computer returned an invalid file chunk."
                    }
                    output.write(bytes)
                    offset += bytes.size
                } while (offset < file.size)
                output.fd.sync()
            }
            if (!partial.renameTo(destination)) throw IOException("Couldn't save the downloaded file.")
        } finally { partial.delete() }
    }

    /**
     * A file a bot shared (iOS `FileDownload.save`), read chunk by chunk into its own new `folder`,
     * checked against its SHA-256, then moved to `folder/ready/<name>`. Any failure removes the folder.
     */
    suspend fun downloadShared(entryId: String, file: SharedFile, folder: File, progress: (Long) -> Unit = {}): File = withContext(Dispatchers.IO) {
        file.validate()
        if (!folder.mkdirs()) throw IOException("Couldn't prepare the download.")
        var saved = false
        try {
            val partial = File(folder, ".download-partial")
            val hash = MessageDigest.getInstance("SHA-256")
            partial.outputStream().use { output ->
                var offset = 0L
                do {
                    ensureActive()
                    val chunk = call("readFile", buildJsonObject { put("entryId", entryId); put("fileId", file.id); put("offset", offset) }, 60_000).jsonObject
                    val data = chunk["data"]?.jsonPrimitive?.contentOrNull
                    val bytes = if (chunk["size"]?.jsonPrimitive?.longOrNull == file.size && data != null && data.length <= CHUNK_BYTES / 3 * 4)
                        try { Base64.getDecoder().decode(data) } catch (_: IllegalArgumentException) { null } else null
                    if (bytes == null || bytes.size.toLong() != minOf(CHUNK_BYTES.toLong(), file.size - offset)) {
                        throw IOException("The download was truncated or changed. Retry the download.")
                    }
                    output.write(bytes)
                    hash.update(bytes)
                    offset += bytes.size
                    progress(offset)
                } while (offset < file.size)
                output.fd.sync()
            }
            if (hash.digest().joinToString("") { "%02x".format(it) } != file.sha256) throw IOException("The file integrity check failed. Retry the download.")
            ensureActive()
            // A name like .download-partial stays valid: the finished file lives in its own folder.
            val destination = File(File(folder, "ready").apply { mkdirs() }, file.name)
            if (!partial.renameTo(destination)) throw IOException("Couldn't save the downloaded file.")
            saved = true
            destination
        } finally { if (!saved) folder.deleteRecursively() }
    }

    /**
     * An artifact's newest HTML (`readArtifact`), chunk by chunk and checked against its SHA-256. An artifact published again while
     * it was read is read again, a few times at most.
     */
    suspend fun readArtifact(botId: String, artifactId: String): Pair<Artifact, String> {
        repeat(ARTIFACT_ATTEMPTS) { readArtifactOnce(botId, artifactId)?.let { return it } }
        throw IOException("The artifact kept changing while it was read. Try again.")
    }

    /** Null when the artifact changed between chunks. */
    private suspend fun readArtifactOnce(botId: String, artifactId: String): Pair<Artifact, String>? {
        val bytes = java.io.ByteArrayOutputStream()
        var first: Artifact? = null
        do {
            val chunk = call("readArtifact", buildJsonObject { put("botId", botId); put("artifactId", artifactId); put("offset", bytes.size()) }, 60_000).jsonObject
            val artifact = Artifact.decode(chunk.getValue("artifact"))
            if (first != null && artifact.updatedAt != first.updatedAt) return null
            first = artifact
            val data = chunk["data"]?.jsonPrimitive?.contentOrNull?.let { try { Base64.getDecoder().decode(it) } catch (_: IllegalArgumentException) { null } }
            require(data != null && chunk["size"]?.jsonPrimitive?.longOrNull == artifact.size && data.size <= CHUNK_BYTES &&
                bytes.size() + data.size <= artifact.size && (data.isNotEmpty() || artifact.size == 0L)) { "The computer returned an invalid artifact chunk." }
            bytes.write(data)
        } while (bytes.size() < artifact.size)
        val artifact = checkNotNull(first)
        val digest = MessageDigest.getInstance("SHA-256").digest(bytes.toByteArray()).joinToString("") { "%02x".format(it) }
        return if (digest == artifact.sha256) artifact to bytes.toString(Charsets.UTF_8) else null
    }

    /** A small text file the host prepared (a memory export), read chunk by chunk. */
    suspend fun readText(botId: String, id: String): String {
        val bytes = java.io.ByteArrayOutputStream()
        var offset = 0L
        do {
            val result = call("readUpload", buildJsonObject { put("botId", botId); put("uploadId", id); put("offset", offset) }, 60_000).jsonObject
            val size = result.getValue("size").jsonPrimitive.long
            val chunk = Base64.getDecoder().decode(result.getValue("data").jsonPrimitive.content)
            require(size in 0..MAX_FILE_BYTES && chunk.size <= CHUNK_BYTES && offset + chunk.size <= size && (chunk.isNotEmpty() || size == 0L)) {
                "The computer returned an invalid file chunk."
            }
            bytes.write(chunk)
            offset += chunk.size
        } while (offset < size)
        return bytes.toString(Charsets.UTF_8)
    }

    /** Text too big for one call (a memory backup), sent as an upload. */
    suspend fun uploadText(botId: String, id: String, name: String, text: String) {
        val data = text.toByteArray()
        require(data.size <= MAX_FILE_BYTES) { "This backup is too large." }
        var offset = 0
        do {
            val end = minOf(offset + CHUNK_BYTES, data.size)
            call("upload", buildJsonObject {
                put("botId", botId); put("uploadId", id); put("name", name); put("offset", offset)
                put("data", Base64.getEncoder().encodeToString(data.copyOfRange(offset, end))); put("done", end == data.size)
            }, 60_000)
            offset = end
        } while (offset < data.size)
    }

    companion object {
        const val CHUNK_BYTES = 384 * 1024
        private const val ARTIFACT_ATTEMPTS = 3
    }
}
