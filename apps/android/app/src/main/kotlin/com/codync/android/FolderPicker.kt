package com.codync.android

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.core.DirListing
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException

/** Browses folders on the computer, drilling down in place (iOS `FolderPicker`). */
@Composable internal fun FolderPicker(start: String?, online: Boolean, load: suspend (String?) -> DirListing, pick: (String) -> Unit) {
    var stack by remember { mutableStateOf(listOf(start)) }
    var forward by remember { mutableStateOf(true) }
    BackHandler(stack.size > 1) { forward = false; stack = stack.dropLast(1) }
    Column(Modifier.fillMaxSize()) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (stack.size > 1) Box(Modifier.padding(start = 12.dp)) { BackButton { forward = false; stack = stack.dropLast(1) } }
            Box(Modifier.weight(1f)) { ModalHeader(stack.last()?.trimEnd('/')?.substringAfterLast('/')?.ifEmpty { "/" } ?: "Folders") }
        }
        AnimatedContent(stack, transitionSpec = {
            val sign = if (forward) 1 else -1
            slideInHorizontally(Motion.layoutOffset) { it * sign } togetherWith slideOutHorizontally(Motion.layoutOffset) { -it * sign }
        }, label = "Folder") { shown ->
            FolderLevel(shown.last(), online, load, pick) { forward = true; stack = stack + it }
        }
    }
}

/** One folder's subfolders, filterable, with "Use" on top. */
@Composable private fun FolderLevel(path: String?, online: Boolean, load: suspend (String?) -> DirListing, pick: (String) -> Unit, open: (String) -> Unit) {
    var listing by remember { mutableStateOf<DirListing?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var filter by remember { mutableStateOf("") }
    LaunchedEffect(path, online) {
        if (!online) { error = "Connect to this computer to browse its folders."; return@LaunchedEffect }
        try { listing = load(path); error = null }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't open this folder." }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(22.dp)) {
        SearchField(filter, { filter = it })
        val current = listing
        when {
            current != null -> {
                CardSection(footer = current.path) {
                    row {
                        Row(Modifier.fillMaxWidth().plainButton { pick(current.path) }, verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Symbol("checkmark.circle.fill", size = 20.dp, tint = Palette.accent)
                            Text("Use “${current.path.trimEnd('/').substringAfterLast('/').ifEmpty { "/" }}”", style = Type.headline, color = Palette.accent)
                        }
                    }
                }
                val dirs = current.dirs.filter { filter.isBlank() || it.name.contains(filter.trim(), ignoreCase = true) }
                if (dirs.isNotEmpty()) CardSection {
                    for (dir in dirs) row {
                        Row(Modifier.fillMaxWidth().plainButton { open(dir.path) }, verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Box(Modifier.width(20.dp), contentAlignment = Alignment.Center) {
                                Symbol(if (dir.isGit) "arrow.triangle.branch" else "folder", size = 18.dp, tint = if (dir.isGit) Palette.accent else Palette.secondary)
                            }
                            Text(dir.name, Modifier.weight(1f), style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
                        }
                    }
                }
            }
            error != null -> Text(tr(error.orEmpty()), style = Type.body, color = Palette.danger)
            else -> Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { Spinner(20.dp) }
        }
    }
}
