package com.codync.android

import android.graphics.Bitmap
import androidx.core.content.FileProvider
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.MAX_FILE_BYTES
import java.io.File
import java.io.RandomAccessFile
import java.util.UUID
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.*
import org.junit.Test

class AttachmentFilesTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val root = File(context.noBackupFilesDir, "files-test-" + UUID.randomUUID()).apply { mkdirs() }
    private val shared = File(context.cacheDir, "shared-attachments/test-" + UUID.randomUUID()).apply { mkdirs() }
    private val files = AttachmentFiles(context, root)
    private fun uri(file: File) = FileProvider.getUriForFile(context, context.packageName + ".files", file)
    @After fun cleanup() { root.deleteRecursively(); shared.deleteRecursively() }

    @Test fun aPickedFileIsCopiedBeforeTheProviderGrantCanDisappear() = runBlocking {
        val source = File(shared, "data.txt").apply { writeText("content") }
        val outgoing = files.import(uri(source))
        source.delete()
        assertEquals("data.txt", outgoing.name)
        assertEquals("content", File(outgoing.path).readText())
        files.discard(outgoing)
        assertFalse(File(outgoing.path).exists())
    }

    @Test fun oversizedProviderFilesAreRejectedBeforeCopying() = runBlocking {
        val source = File(shared, "too-large.bin")
        RandomAccessFile(source, "rw").use { it.setLength(MAX_FILE_BYTES + 1) }
        try { files.import(uri(source)); fail("Expected size validation") }
        catch (_: IllegalArgumentException) { }
        assertTrue(File(root, "outgoing").listFiles().orEmpty().isEmpty())
    }

    @Test fun supportedNonPortableImagesBecomeBoundedJpegFiles() = runBlocking {
        val source = File(shared, "photo.bmp")
        // Android detects image format from its bytes; the nonportable name selects preparation.
        source.outputStream().use { Bitmap.createBitmap(16, 8, Bitmap.Config.ARGB_8888).compress(Bitmap.CompressFormat.PNG, 100, it) }
        val outgoing = files.import(uri(source))
        assertEquals("photo.jpg", outgoing.name)
        val bitmap = AttachmentFiles.thumbnail(File(outgoing.path), 4096)
        assertEquals(16, bitmap.width); assertEquals(8, bitmap.height)
    }

    @Test fun fileSharingCannotExposePrivateIdentityOrMirrorStorage() {
        val private = File(root, "identity").apply { writeText("private") }
        try { uri(private); fail("Private storage was exposed") }
        catch (_: IllegalArgumentException) { }
    }
}
