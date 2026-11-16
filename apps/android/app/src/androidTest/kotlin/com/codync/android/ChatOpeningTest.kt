package com.codync.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ChatOpeningTest {
    @Test fun openBotStartsItsWaitAndItsFirstMessagesFinishItOnce() {
        val store = previewStore()
        store.openBot("b1")
        assertNotNull(store.chatOpening.finish("b1"))
        assertNull(store.chatOpening.finish("b1"))
        store.openBot("b1")
        assertNull(store.chatOpening.finish("b1"))
        store.openBot("b2")
        assertNull(store.chatOpening.finish("b1"))
        assertNotNull(store.chatOpening.finish("b2"))
        store.openBot("b2")
        store.openBot(null)
        assertNull(store.chatOpening.finish("b2"))
    }
}
