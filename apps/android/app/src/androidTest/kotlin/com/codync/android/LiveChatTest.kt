package com.codync.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.util.UUID
import java.io.File
import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import android.content.ClipData
import android.content.ClipboardManager
import androidx.core.content.FileProvider
import androidx.lifecycle.ViewModelProvider
import com.codync.android.core.*
import kotlinx.serialization.json.*
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals

/** Opt-in: the paired, isolated acceptance host uses host/tests/fake_agent.py. */
class LiveChatTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun aRealHostSendPermissionAndReplyRoundTrip() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("liveHost") == "true")
        val bot = "Android chat acceptance"
        compose.waitUntil(30_000) { compose.onAllNodesWithText(bot).fetchSemanticsNodes().isNotEmpty() }
        val back = compose.onAllNodes(hasText("Back") or hasContentDescription("Back")).fetchSemanticsNodes()
        if (back.isEmpty()) compose.onNodeWithText(bot).performClick()
        compose.waitUntil(10_000) { compose.onAllNodes(chatTitle).fetchSemanticsNodes().size == 1 }
        val message = "Android acceptance " + UUID.randomUUID()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val local = LocalStore(context)
        requireFixtureHost(local.computers().single().id)
        val mirror = MirrorDatabase(context, local.directory, local.computers().single().id)
        compose.onNode(hasSetTextAction()).performTextReplacement(message)
        compose.onNodeWithContentDescription("Send").performClick()
        compose.waitUntil(30_000) { compose.onAllNodesWithText("Allow once").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Allow once").performScrollTo().performClick()
        try {
            compose.waitUntil(30_000) {
                val entries = mirror.snapshot().entries
                val sent = entries.firstOrNull { it.kind == "user" && it.text == message }
                sent != null && entries.any { it.seq > sent.seq && it.kind == "agent" && it.isChat }
            }
            compose.onAllNodes(hasText("Done:", substring = true)).onLast().assertIsDisplayed()
            if (InstrumentationRegistry.getArguments().getString("layoutSnapshots") == "true") {
                val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
                File(context.cacheDir, "layout-chat.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
            }
            val source = File(context.cacheDir, "shared-attachments/live-" + UUID.randomUUID() + ".bin").apply { parentFile?.mkdirs() }
            val bytes = ByteArray(HostFiles.CHUNK_BYTES + 17) { (it % 255).toByte() }
            source.writeBytes(bytes)
            try {
                compose.runOnUiThread {
                    val uri = FileProvider.getUriForFile(context, context.packageName + ".files", source)
                    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newUri(context.contentResolver, "Acceptance file", uri))
                }
                compose.onNodeWithContentDescription("Add files").performClick()
                compose.onNodeWithText("Paste").performClick()
                compose.waitUntil(10_000) { compose.onAllNodes(hasText(source.name, substring = true)).fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithContentDescription("Send").performClick()
                compose.waitUntil(30_000) { compose.onAllNodesWithText("Allow once").fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithText("Allow once").performScrollTo().performClick()
                var sent: Entry? = null
                compose.waitUntil(30_000) {
                    val entries = mirror.snapshot().entries
                    sent = entries.firstOrNull { entry -> entry.kind == "user" && entry.data["attachments"]?.jsonArray?.any { it.jsonObject["name"]?.jsonPrimitive?.content == source.name } == true }
                    val upload = sent
                    upload != null && entries.any { it.seq > upload.seq && it.kind == "agent" && it.isChat }
                }
                val entry = requireNotNull(sent)
                val attachment = WireJson.decodeFromJsonElement<Attachment>(entry.data.getValue("attachments").jsonArray.single())
                runBlocking {
                    val computer = local.computers().single().copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly)
                    val channel = HostConnector.connect(computer, local.identity())
                    val downloaded = File(context.cacheDir, "live-download-" + UUID.randomUUID())
                    try {
                        HostFiles(channel).download(entry.botId, attachment, downloaded)
                        assertArrayEquals(bytes, downloaded.readBytes())
                    } finally { channel.close(); downloaded.delete() }
                }
            } finally {
                source.delete()
                compose.runOnUiThread { context.getSystemService(ClipboardManager::class.java).clearPrimaryClip() }
            }
        } finally { mirror.close() }
    }
}
