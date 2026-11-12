package com.codync.android

import android.content.ClipboardManager
import android.graphics.Bitmap
import android.os.Build
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import java.util.UUID
import kotlinx.serialization.json.*
import org.junit.After
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test

/** A sent picture full size: its menus copy the picture or its name, it zooms, and zoomed in a drag doesn't close it. */
class ImageViewerTest {
    @get:Rule val compose = createComposeRule()
    private val store = previewStore()
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val clipboard = context.getSystemService(ClipboardManager::class.java)
    private val name = "chart.png"
    private val id = UUID.randomUUID().toString()
    private val sharedId = UUID.randomUUID().toString()
    private val sharedName = "plot.png"
    private lateinit var bytes: ByteArray

    @Before fun seedThePicture() {
        assumeTrue(Build.PRODUCT.startsWith("sdk"))
        // Already in the cache, so the store never asks the (absent) computer for it.
        val file = store.attachments.cached(id)
        file.outputStream().use { out ->
            Bitmap.createBitmap(1200, 800, Bitmap.Config.ARGB_8888).apply { eraseColor(0xFF3366CC.toInt()) }.compress(Bitmap.CompressFormat.PNG, 100, out)
        }
        bytes = file.readBytes()
        file.copyTo(store.attachments.cached(sharedId), overwrite = true)
        val bot = Bot("image-preview", "Scout")
        val attachment = WireJson.encodeToJsonElement(Attachment.serializer(), Attachment(id, name, bytes.size.toLong()))
        val entry = Entry("sent", 1, bot.id, rev = 1, kind = "user", data = buildJsonObject {
            put("text", ""); put("attachments", buildJsonArray { add(attachment) })
        })
        val shared = WireJson.encodeToJsonElement(SharedFile.serializer(), SharedFile(sharedId, sharedName, bytes.size.toLong(), "0".repeat(64)))
        val reply = Entry("shared", 2, bot.id, rev = 2, kind = "agent", data = buildJsonObject {
            put("text", "Sent $sharedName"); put("final", true); put("files", buildJsonArray { add(shared) })
        })
        val state = AppState(previewComputer, loading = false, selectedBot = bot.id, bots = listOf(bot), connection = LinkState.Ready(HostRoute.Direct),
            historyEnd = mapOf(bot.id to HistoryEnd.Complete), entries = listOf(entry, reply))
        compose.setContent { Hosted { ChatScreen(state, store, {}) } }
        // The list scrolls to its end as it first lays out: touches wait for that.
        compose.waitForIdle()
    }

    @After fun cleanup() {
        store.attachments.cached(id).delete()
        store.attachments.cached(sharedId).delete()
        compose.runOnUiThread { clipboard.clearPrimaryClip() }
    }

    private val thumbnail get() = compose.onAllNodesWithContentDescription(name).onFirst()
    /** The open viewer's picture: drawn over the chat, so after its thumbnail. */
    private val picture get() = compose.onAllNodesWithContentDescription(name).onLast()
    private fun viewerOpen() = compose.onAllNodesWithContentDescription("Copy image").fetchSemanticsNodes().isNotEmpty()

    /** The viewer's colour at a fraction of its width and height: the seeded blue where the picture is. */
    private fun colorAt(x: Float, y: Float): Int = picture.captureToImage().asAndroidBitmap().let { it.getPixel((it.width * x).toInt(), (it.height * y).toInt()) }
    private val blue = 0xFF3366CC.toInt()
    /** Above the picture at fit (a wide picture leaves room over it), inside it once zoomed in 2.5x. */
    private fun zoomedIn() = colorAt(0.5f, 0.15f) == blue

    private fun openViewer() {
        thumbnail.performClick()
        compose.waitUntil(15_000) { viewerOpen() && colorAt(0.5f, 0.5f) == blue }
    }

    @Test fun holdingASentPictureOffersCopyImageAndCopyNameInPlaceOfCopy() {
        thumbnail.performTouchInput { longClick() }
        compose.onNodeWithContentDescription("👍").assertIsDisplayed()
        compose.onNodeWithText("Copy image").assertIsDisplayed()
        compose.onNodeWithText("Reply").assertIsDisplayed()
        compose.onNodeWithText("Copy").assertDoesNotExist()
        // A picture copy still converting when Copy name is chosen must not land over the name.
        val converting = store.imageCopies.start()
        compose.onNodeWithText("Copy name").performClick()
        compose.runOnIdle { assertEquals(name, clipboard.primaryClip?.getItemAt(0)?.text?.toString()) }
        assertFalse(store.imageCopies.isLatest(converting))
    }

    @Test fun theViewerCopiesThePictureItselfThenShowsCopied() {
        openViewer()
        compose.onNodeWithContentDescription("Copy image").performClick()
        compose.waitUntil(5_000) { compose.onAllNodesWithContentDescription("Copied").fetchSemanticsNodes().isNotEmpty() }
        val uri = compose.runOnIdle { clipboard.primaryClip?.getItemAt(0)?.uri }
        assertNotNull(uri)
        assertEquals("image/png", context.contentResolver.getType(uri!!))
        assertArrayEquals(bytes, context.contentResolver.openInputStream(uri)!!.use { it.readBytes() })
        // The check mark gives way to the button again.
        compose.waitUntil(5_000) { viewerOpen() }
    }

    @Test fun aCopyStartedInTheViewerFinishesAfterItCloses() {
        openViewer()
        compose.onNodeWithContentDescription("Copy image").performClick()
        compose.onNodeWithContentDescription("Close").performClick()
        compose.waitUntil(5_000) { clipboard.primaryClip?.getItemAt(0)?.uri != null }
    }

    @Test fun zoomedInADragMovesThePictureAndAtFitItClosesTheViewer() {
        openViewer()
        assertFalse(zoomedIn())
        picture.performTouchInput { doubleClick(center) }
        compose.waitForIdle()
        assertTrue(zoomedIn())
        picture.performTouchInput { swipeDown(startY = centerY, endY = bottom) }
        compose.waitForIdle()
        assertTrue(viewerOpen())
        assertTrue(zoomedIn())
        // Spreading two fingers keeps it zoomed in; a double tap goes back to fit, where a drag down closes the sheet.
        picture.performTouchInput { pinch(center - Offset(50f, 0f), center - Offset(250f, 0f), center + Offset(50f, 0f), center + Offset(250f, 0f)) }
        picture.performTouchInput { swipeDown(startY = centerY, endY = bottom) }
        compose.waitForIdle()
        assertTrue(viewerOpen())
        assertTrue(zoomedIn())
        picture.performTouchInput { doubleClick(center) }
        compose.waitForIdle()
        assertFalse(zoomedIn())
        picture.performTouchInput { swipeDown(startY = centerY, endY = bottom) }
        compose.waitUntil(5_000) { !viewerOpen() }
    }

    @Test fun holdingThePictureInTheViewerOffersItsMessageMenuAndReplyClosesTheViewer() {
        openViewer()
        picture.performTouchInput { longClick() }
        compose.onNodeWithText("Copy name").assertIsDisplayed()
        // A reaction keeps the viewer open.
        compose.onNodeWithContentDescription("👍").performClick()
        compose.waitForIdle()
        assertTrue(viewerOpen())
        // The menu's Copy image flashes the header button, as tapping it does.
        picture.performTouchInput { longClick() }
        compose.onNodeWithText("Copy image").performClick()
        compose.waitUntil(5_000) { compose.onAllNodesWithContentDescription("Copied").fetchSemanticsNodes().isNotEmpty() }
        compose.waitUntil(5_000) { viewerOpen() }
        picture.performTouchInput { longClick() }
        compose.onNodeWithText("Reply").performClick()
        compose.waitUntil(5_000) { !viewerOpen() }
        compose.onNodeWithText("Reply…").assertIsDisplayed()
    }

    @Test fun aPictureABotSharedHasItsDownloadButtonByTheCloseButtonAndShowWhatItDid() {
        compose.onAllNodesWithContentDescription(sharedName).onFirst().performTouchInput { longClick() }
        compose.onNodeWithText("Copy image").assertIsDisplayed()
        compose.onNodeWithText("Show what it did").assertIsDisplayed()
        compose.onNodeWithText("Copy").assertDoesNotExist()
        compose.onNodeWithText("Copy name").performClick()
        compose.runOnIdle { assertEquals(sharedName, clipboard.primaryClip?.getItemAt(0)?.text?.toString()) }
        compose.onAllNodesWithContentDescription(sharedName).onFirst().performClick()
        compose.waitUntil(15_000) { viewerOpen() }
        // The card's button and the viewer's, by its close button.
        compose.onAllNodesWithContentDescription("Download $sharedName").assertCountEquals(2)
        compose.onNodeWithContentDescription("Close").assertIsDisplayed()
    }
}
