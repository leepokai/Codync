package com.codync.android

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.net.Uri
import android.provider.OpenableColumns
import androidx.core.content.FileProvider
import com.codync.android.core.*
import java.io.File
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import kotlin.coroutines.coroutineContext

/** Copies provider streams into this account's private storage before it owns a send. */
class AttachmentFiles(private val context: Context, private val directory: File) {
    private val outgoing = File(directory, "outgoing").apply { mkdirs() }
    private val downloads = File(directory, "attachments").apply { mkdirs() }

    suspend fun import(uri: Uri): OutgoingFile = withContext(Dispatchers.IO) {
        check(outgoing.mkdirs() || outgoing.isDirectory)
        var name = "file"
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) {
                name = cursor.getString(0) ?: name
                if (!cursor.isNull(1)) require(cursor.getLong(1) <= MAX_FILE_BYTES) { "Files must be 100 MiB or smaller." }
            }
        }
        name = cleanName(name)
        val used = outgoing.listFiles().orEmpty().sumOf { it.length() }
        require(outgoing.listFiles().orEmpty().size < 200 && used < 512L * 1024 * 1024) {
            "Attachment storage is full. Send or remove older files first."
        }
        val destination = File(outgoing, UUID.randomUUID().toString())
        try {
            val input = requireNotNull(context.contentResolver.openInputStream(uri)) { "Couldn't open the selected file." }
            input.use { source -> destination.outputStream().use { target ->
                val buffer = ByteArray(64 * 1024)
                var total = 0L
                while (true) {
                    coroutineContext.ensureActive()
                    val count = source.read(buffer)
                    if (count < 0) break
                    total += count
                    require(total <= MAX_FILE_BYTES) { "Files must be 100 MiB or smaller." }
                    require(used + total <= 512L * 1024 * 1024) { "Attachment storage is full. Send or remove older files first." }
                    target.write(buffer, 0, count)
                }
                target.fd.sync()
            } }
            if (name.substringAfterLast('.', "").lowercase() in listOf("heic", "heif", "bmp")) {
                val bitmap = thumbnail(destination, 4096)
                val converted = File(outgoing, UUID.randomUUID().toString())
                try {
                    converted.outputStream().use { target -> check(bitmap.compress(Bitmap.CompressFormat.JPEG, 85, target)); target.fd.sync() }
                    destination.delete()
                    return@withContext OutgoingFile(converted.path, name.substringBeforeLast('.') + ".jpg", converted.length())
                } catch (error: Exception) { converted.delete(); throw error }
            }
            OutgoingFile(destination.path, name, destination.length())
        } catch (error: Throwable) { destination.delete(); throw error }
    }

    fun discard(file: OutgoingFile) {
        val path = File(file.path)
        if (path.parentFile?.canonicalFile == outgoing.canonicalFile) path.delete()
    }
    fun retainPending(files: List<OutgoingFile>) {
        val paths = files.map { File(it.path).canonicalPath }.toSet()
        outgoing.listFiles().orEmpty().filter { it.canonicalPath !in paths }.forEach { it.delete() }
    }
    fun cached(id: String): File {
        check(downloads.mkdirs() || downloads.isDirectory)
        val uuid = UUID.fromString(id).toString()
        require(uuid == id.lowercase()) { "Invalid attachment ID." }
        return File(downloads, uuid)
    }
    fun cache(id: String, source: OutgoingFile) {
        File(source.path).copyTo(cached(id), overwrite = true)
        trim()
    }
    fun trim() {
        val files = downloads.listFiles().orEmpty().filter { it.isFile && !it.name.endsWith(".partial") }.sortedBy { it.lastModified() }
        var bytes = files.sumOf { it.length() }
        for (file in files) if (bytes > 512L * 1024 * 1024) { bytes -= file.length(); file.delete() }
    }

    /**
     * A new folder for one file the share sheet hands out. The receiving app reads it after the sheet
     * closes, so folders are cleared only once they're a day old.
     */
    fun shareFolder(): File {
        val root = File(context.cacheDir, "shared-attachments").apply { mkdirs() }
        root.listFiles().orEmpty().filter { System.currentTimeMillis() - it.lastModified() > 24 * 60 * 60 * 1000 }.forEach { it.deleteRecursively() }
        return File(root, UUID.randomUUID().toString())
    }

    /** The system share sheet for a file in a `shareFolder`. */
    fun open(shared: File, name: String) {
        val uri = FileProvider.getUriForFile(context, context.packageName + ".files", shared)
        val type = android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(shared.extension.lowercase()) ?: "application/octet-stream"
        val intent = Intent(Intent.ACTION_SEND).setType(type).putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        context.startActivity(Intent.createChooser(intent, "Share $name").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    companion object {
        fun cleanName(value: String): String {
            val name = value.substringAfterLast('/').substringAfterLast('\\').trim().trimStart('.').filter { !it.isISOControl() }
            val usable = name.ifBlank { "file" }
            require(usable.toByteArray(Charsets.UTF_8).size <= 200) { "The file name is too long." }
            return usable
        }
        fun thumbnail(file: File, maxPixels: Int): Bitmap = ImageDecoder.decodeBitmap(ImageDecoder.createSource(file)) { decoder, info, _ ->
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            val longest = maxOf(info.size.width, info.size.height)
            decoder.setTargetSampleSize(maxOf(1, (longest + maxPixels - 1) / maxPixels))
        }
    }
}
