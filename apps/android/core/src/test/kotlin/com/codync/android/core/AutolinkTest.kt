package com.codync.android.core

import org.junit.Assert.assertEquals
import org.junit.Test

/** The same cases as the iPhone's, desktop's and terminal's autolink tests. */
class AutolinkTest {
    private fun urls(text: String) = Autolink.find(text).map { it.url }

    @Test fun `finds bare addresses and leaves the sentence around them`() {
        assertEquals(listOf("https://codync.dev"), urls("see https://codync.dev."))
        assertEquals(listOf("https://www.example.com/a?b=1"), urls("open www.example.com/a?b=1, then"))
        assertEquals(listOf("https://example.com/x"), urls("(at https://example.com/x)"))
        assertEquals(listOf("https://en.wikipedia.org/wiki/Rust_(language)"), urls("https://en.wikipedia.org/wiki/Rust_(language)"))
        assertEquals(listOf("https://codync.dev"), urls("網址：https://codync.dev，謝謝"))
        assertEquals(listOf("HTTP://A.COM", "http://b.io/x_y_z"), urls("HTTP://A.COM and http://b.io/x_y_z"))
    }

    @Test fun `skips what only looks like an address`() {
        assertEquals(emptyList<String>(), urls("main.rs README.md codync.dev"))
        assertEquals(emptyList<String>(), urls("xhttps://a.com nohttp"))
        assertEquals(emptyList<String>(), urls("https:// and www."))
    }

    @Test fun `keeps where the address is`() {
        assertEquals(listOf(Autolink.Found(2..12, "https://b.c")), Autolink.find("a https://b.c d"))
    }
}
