package com.codync.android.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AttachmentKindsTest {
    @Test fun `pictures and clips are told apart by name, case aside`() {
        assertTrue(Attachment("a", "Photo.JPG", 10).isImage)
        assertTrue(Attachment("a", "map.svg", 10).isImage)
        assertFalse(Attachment("a", "map.svg", 10).isVideo)
        assertTrue(Attachment("a", "clip.MP4", 10).isVideo)
        assertFalse(Attachment("a", "notes.pdf", 10).isImage)
        assertFalse(Attachment("a", "notes.pdf", 10).isVideo)
    }

    @Test fun `a shared SVG shows as an image but is not decoded as a raster, and big pictures fall back to a card`() {
        val svg = SharedFile("a", "icon.svg", 10, "0".repeat(64))
        assertTrue(svg.isImage)
        assertFalse(svg.showsAsImage)
        assertTrue(svg.isSvg)
        assertTrue(SharedFile("a", "photo.png", 10 * 1024 * 1024, "0".repeat(64)).showsAsImage)
        assertFalse(SharedFile("a", "huge.png", 21L * 1024 * 1024, "0".repeat(64)).showsAsImage)
        assertEquals(true, SharedFile("a", "clip.mp4", 10, "0".repeat(64)).isVideo)
    }
}
