package com.codync.android

import org.junit.Assert.*
import org.junit.Test

class ComposerAttachmentTest {
    @Test fun `paste is offered for images and other uris, not for text`() {
        assertTrue(clipOffersFiles(listOf("image/png")))
        assertTrue(clipOffersFiles(listOf("application/pdf")))
        assertTrue(clipOffersFiles(listOf("text/uri-list")))
        assertFalse(clipOffersFiles(listOf("text/plain")))
        assertFalse(clipOffersFiles(listOf("text/plain", "text/html")))
        assertFalse(clipOffersFiles(emptyList()))
    }

    @Test fun `file chips use the symbol of the file's kind`() {
        assertEquals("photo", AttachmentIcon.symbol("shot.PNG"))
        assertEquals("doc.richtext", AttachmentIcon.symbol("paper.pdf"))
        assertEquals("doc.zipper", AttachmentIcon.symbol("logs.tar"))
        assertEquals("film", AttachmentIcon.symbol("clip.mov"))
        assertEquals("waveform", AttachmentIcon.symbol("memo.m4a"))
        assertEquals("doc", AttachmentIcon.symbol("README"))
    }
}
