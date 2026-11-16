package com.codync.android

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ChatOpeningTest {
    @get:Rule val compose = createComposeRule()

    @Test fun placeholderOnlyLayoutWaitsForAMessageToFinishOpening() {
        compose.mainClock.autoAdvance = false
        val rows = mutableStateOf(listOf("intro"))
        var settled = 0
        var opened = 0
        compose.setContent {
            val list = rememberLazyListState()
            rememberFollowing(list, rows.value.lastOrNull()?.takeUnless { it == "intro" }, null,
                working = false, bottom = 0.dp, onSettled = { settled++ }, onOpened = { opened++ })
            LazyColumn(Modifier.size(320.dp), state = list) {
                items(rows.value, key = { it }) { Box(Modifier.size(20.dp)) }
            }
        }
        compose.mainClock.advanceTimeBy(1000)
        compose.runOnIdle {
            assertEquals(1, settled)
            assertEquals(0, opened)
            rows.value = listOf("intro", "message")
        }
        compose.mainClock.advanceTimeBy(1000)
        compose.runOnIdle { assertEquals(1, opened) }
    }

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
