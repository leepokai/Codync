package com.codync.android

import org.junit.Assert.*
import org.junit.Test

class ComposerAttachmentTest {
    @Test fun `file chips use the symbol of the file's kind`() {
        assertEquals("photo", attachmentSymbol("shot.PNG"))
        assertEquals("doc.richtext", attachmentSymbol("paper.pdf"))
        assertEquals("doc.zipper", attachmentSymbol("logs.tar"))
        assertEquals("film", attachmentSymbol("clip.mov"))
        assertEquals("waveform", attachmentSymbol("memo.m4a"))
        assertEquals("doc", attachmentSymbol("README"))
    }
}
