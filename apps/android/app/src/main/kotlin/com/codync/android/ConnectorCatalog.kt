package com.codync.android

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.codync.android.core.CatalogPage
import kotlinx.coroutines.CancellationException

/**
 * The connector list Extensions scrolls through (iOS `ConnectorCatalog`): the rows loaded so far and
 * the registry cursor. The end of the list coming into view asks for more (`loadMore`).
 */
internal class ConnectorCatalog<T>(
    private val id: (T) -> String,
    private val fetch: suspend (search: String, cursor: String?) -> Pair<List<T>, String?>,
) {
    var items by mutableStateOf(CatalogPage<T>()); private set
    /** The registry page after the ones loaded; null at the end. */
    var cursor by mutableStateOf<String?>(null); private set
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
            items = CatalogPage<T>().append(rows, id); cursor = next?.takeIf(String::isNotBlank); error = null
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) {
            if (request != mine) return
            items = CatalogPage(); cursor = null; error = failure.message ?: "Couldn't load connectors."
        } finally { if (request == mine) loading = false }
    }

    /**
     * The registry page at the cursor. It can bring nothing Codync can install; the cursor still moves on, so
     * `LoadMoreRow` asks again. A failure keeps the cursor and shows under the list.
     */
    suspend fun loadMore() {
        val at = cursor
        if (loading || loadingMore || at == null) return
        val mine = request
        loadingMore = true; error = null
        try {
            val (rows, next) = fetch(query, at)
            if (request != mine) return
            items = items.append(rows, id); cursor = next?.takeIf(String::isNotBlank)
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) {
            if (request != mine) return
            error = failure.message ?: "Couldn't load more connectors."
        } finally { if (request == mine) loadingMore = false }
    }
}
