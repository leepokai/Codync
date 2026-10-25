package com.codync.android

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.codync.android.core.CatalogPage
import com.codync.android.core.CatalogPaging
import kotlinx.coroutines.CancellationException

/**
 * The connector list Extensions pages through (iOS `ConnectorCatalog`): the rows loaded so far, the
 * registry cursor and the page on screen. A page that isn't full asks the registry until it is, or
 * until the cursor ends.
 */
internal class ConnectorCatalog<T>(
    private val id: (T) -> String,
    private val fetch: suspend (search: String, cursor: String?) -> Pair<List<T>, String?>,
) {
    var items by mutableStateOf(CatalogPage<T>()); private set
    /** The registry page after the ones loaded; null at the end. */
    var cursor by mutableStateOf<String?>(null); private set
    /** The page on screen. While the registry fills it, it can be the page past the loaded rows. */
    var page by mutableIntStateOf(0); private set
    var loading by mutableStateOf(true); private set
    var loadingMore by mutableStateOf(false); private set
    var error by mutableStateOf<String?>(null); private set
    private var request = 0
    private var query = ""

    val hasMore: Boolean get() = cursor != null

    /** The first registry page for `search`; answers to an older search are dropped. */
    suspend fun load(search: String) {
        val mine = ++request
        query = search; loading = true; loadingMore = false
        try {
            val (rows, next) = fetch(search, null)
            if (request != mine) return
            items = CatalogPage<T>().append(rows, id); cursor = next?.takeIf(String::isNotBlank); page = 0; error = null
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) {
            if (request != mine) return
            items = CatalogPage(); cursor = null; error = failure.message ?: "Couldn't load connectors."
        } finally { if (request == mine) loading = false }
    }

    /** Turns to `next` at once, so a swipe stays where it landed; `fill` then loads its rows. */
    fun show(next: Int, size: Int) {
        if (!loading) page = CatalogPaging.clamp(next, items.items.size, size, hasMore)
    }

    /**
     * Asks the registry until the page on screen is full or the cursor ends. A page turned meanwhile
     * is filled by the same loop. If a page fails, the list goes back to the last page with rows.
     */
    suspend fun fill(size: Int) {
        if (loading || loadingMore || !hasMore || !CatalogPaging.needsFetch(items.items.size, page, size)) return
        val mine = request
        loadingMore = true
        try {
            while (true) {
                val at = cursor ?: break
                if (!CatalogPaging.needsFetch(items.items.size, page, size)) break
                val (rows, next) = fetch(query, at)
                if (request != mine) return
                items = items.append(rows, id); cursor = next?.takeIf(String::isNotBlank)
            }
            page = CatalogPaging.clamp(page, items.items.size, size, hasMore); error = null
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) {
            if (request != mine) return
            error = failure.message ?: "Couldn't load more connectors."
            page = CatalogPaging.clamp(page, items.items.size, size)
        } finally { if (request == mine) loadingMore = false }
    }
}
