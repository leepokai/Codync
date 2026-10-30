package com.codync.android

import com.codync.android.core.DirListing
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class WorkspacePickerStateTest {
    private fun listing(path: String, recent: Boolean = true) = DirListing(path, dirs = emptyList(), recent = if (recent) emptyList() else null)

    @Test fun `same folder can be reopened and recent closes path input`() = runBlocking {
        val state = WorkspacePickerState()
        state.open("/project")
        state.load(true) { listing(it!!) }
        state.open("/project")
        state.load(true) { listing(it!!) }
        assertEquals("/project", state.selected)
        state.pathOpen = true
        state.showRecent()
        assertFalse(state.pathOpen)
        assertEquals("", state.selected)
    }

    @Test fun `older host validates without calling unsupported selection method`() = runBlocking {
        val state = WorkspacePickerState()
        state.open("/project")
        state.load(true) { listing(it!!, recent = false) }
        val chosen = state.confirm({ listing(it!!, recent = false) }, { error("unsupported endpoint") })
        assertEquals("/project", chosen)
    }

    @Test fun `failed selection stays open and shows the error`() = runBlocking {
        val state = WorkspacePickerState()
        state.open("/gone")
        state.load(true) { listing(it!!) }
        assertNull(state.confirm({ listing(it!!) }, { error("Folder was removed") }))
        assertEquals("Folder was removed", state.error)
        assertFalse(state.saving)
    }

    @Test fun `older folder response cannot replace newer navigation`() = runBlocking {
        val state = WorkspacePickerState()
        val first = CompletableDeferred<DirListing>()
        state.open("/old")
        val pending = launch(start = CoroutineStart.UNDISPATCHED) { state.load(true) { first.await() } }
        state.open("/new")
        state.load(true) { listing(it!!) }
        first.complete(listing("/old"))
        pending.join()
        assertEquals("/new", state.selected)
        assertEquals("/new", state.listing?.path)
    }
}
