package com.codync.android

import android.view.KeyEvent
import org.junit.Assert.*
import org.junit.Test

class ScreenKeysTest {
    @Test fun `special keys and shortcuts go to the computer, plain typing stays with the field`() {
        assertEquals("left" to emptyList<String>(), ScreenKeys.map(KeyEvent.KEYCODE_DPAD_LEFT, 0, 0))
        assertEquals("tab" to listOf("shift"), ScreenKeys.map(KeyEvent.KEYCODE_TAB, KeyEvent.META_SHIFT_ON, 0))
        assertEquals("f12" to emptyList<String>(), ScreenKeys.map(KeyEvent.KEYCODE_F12, 0, 0))
        assertEquals("forwardDelete" to emptyList<String>(), ScreenKeys.map(KeyEvent.KEYCODE_FORWARD_DEL, 0, 0))
        assertEquals("c" to listOf("cmd", "shift"), ScreenKeys.map(KeyEvent.KEYCODE_C, KeyEvent.META_META_ON or KeyEvent.META_SHIFT_ON, 'C'.code))
        assertEquals("a" to listOf("ctrl"), ScreenKeys.map(KeyEvent.KEYCODE_A, KeyEvent.META_CTRL_ON, 'a'.code))
        assertNull("typing goes through the IME", ScreenKeys.map(KeyEvent.KEYCODE_A, KeyEvent.META_SHIFT_ON, 'a'.code))
        assertNull("Ctrl-Space switches the input language", ScreenKeys.map(KeyEvent.KEYCODE_SPACE, KeyEvent.META_CTRL_ON, ' '.code))
        assertNull(ScreenKeys.map(KeyEvent.KEYCODE_VOLUME_UP, KeyEvent.META_CTRL_ON, 0))
    }
}
