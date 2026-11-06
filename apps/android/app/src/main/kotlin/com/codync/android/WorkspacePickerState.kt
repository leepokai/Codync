package com.codync.android

import androidx.compose.runtime.*
import com.codync.android.core.DirListing
import kotlinx.coroutines.CancellationException

internal enum class FolderMode { Recent, Browse }

/** Navigation and request lifetime, independent of the composable that renders it. */
internal class WorkspacePickerState {
    var mode by mutableStateOf(FolderMode.Recent)
        private set
    var path by mutableStateOf<String?>(null)
        private set
    var listing by mutableStateOf<DirListing?>(null)
        private set
    var selected by mutableStateOf("")
    var filter by mutableStateOf("")
    var error by mutableStateOf<String?>(null)
        private set
    var busy by mutableStateOf(true)
        private set
    var saving by mutableStateOf(false)
        private set
    var pathOpen by mutableStateOf(false)
    var pathInput by mutableStateOf("")
    var request by mutableIntStateOf(0)
        private set
    private var generation = 0

    fun open(next: String?) {
        if (saving) return
        mode = FolderMode.Browse
        path = next
        selected = ""
        filter = ""
        pathOpen = false
        request++
    }

    fun showRecent() {
        if (saving) return
        mode = FolderMode.Recent
        path = null
        selected = ""
        filter = ""
        pathOpen = false
        request++
    }

    suspend fun load(online: Boolean, fetch: suspend (String?) -> DirListing) {
        val current = ++generation
        listing = null
        error = null
        busy = true
        try {
            check(online) { "Connect to this computer to browse its folders." }
            val result = fetch(path)
            if (current != generation) return
            listing = result
            if (mode == FolderMode.Browse) selected = result.path
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) {
            if (current == generation) error = failure.message ?: "Couldn't open this folder."
        } finally { if (current == generation) busy = false }
    }

    suspend fun confirm(fetch: suspend (String?) -> DirListing, select: suspend (String) -> String): String? {
        if (saving || busy || selected.isEmpty()) return null
        val current = generation
        saving = true
        error = null
        return try {
            val chosen = if (listing?.recent != null) select(selected) else fetch(selected).path
            chosen.takeIf { current == generation }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't open this folder."; null }
        finally { saving = false }
    }
}
