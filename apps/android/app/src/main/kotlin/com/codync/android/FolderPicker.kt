package com.codync.android

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.DirListing
import com.codync.android.design.*
import kotlinx.coroutines.launch

/** Host-specific recent selections and browsing; never mixes paths from different computers. */
@Composable internal fun FolderPicker(start: String?, online: Boolean, computer: String,
    load: suspend (String?) -> DirListing, select: suspend (String) -> String, pick: (String) -> Unit) {
    val state = remember { WorkspacePickerState() }
    val scope = rememberCoroutineScope()
    val open = state::open
    val recent = state::showRecent
    val noRecent = state.mode == FolderMode.Recent && !state.busy && state.listing?.recent.orEmpty().isEmpty()
    BackHandler(state.mode == FolderMode.Browse) { recent() }
    LaunchedEffect(state.request, online) { state.load(online, load) }
    Column(Modifier.fillMaxSize()) {
        ModalHeader("Choose workspace")
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(tr("Your bot will work in this folder."), style = Type.subheadline, color = Palette.secondary)
            Row(Modifier.fillMaxWidth().background(Palette.surface, RoundedCornerShape(12.dp)).padding(12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Symbol("desktopcomputer", size = 18.dp)
                Text(computer, Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis, style = Type.body)
                Text(tr(if (online) "Connected" else "Disconnected"), style = Type.caption, color = Palette.secondary)
            }
            if (noRecent) FolderBrowse(!state.saving) { open(start) }
            if (state.mode == FolderMode.Browse) Row(verticalAlignment = Alignment.CenterVertically) {
                IconButton("Recent workspaces", "chevron.left", enabled = !state.saving) { recent() }
                Row(Modifier.weight(1f).horizontalScroll(rememberScrollState()), verticalAlignment = Alignment.CenterVertically) {
                    val ancestors = state.listing?.ancestors
                    if (ancestors != null) ancestors.forEachIndexed { i, dir ->
                        if (i > 0) Text("/", color = Palette.tertiary, style = Type.caption)
                        Text(dir.name, Modifier.padding(6.dp).plainButton(enabled = !state.saving) { open(dir.path) }, style = Type.caption, maxLines = 1)
                    } else Text(state.listing?.path ?: tr("Folders"), style = Type.caption, maxLines = 1)
                }
                IconButton("Parent folder", "arrow.up", enabled = state.listing?.parent != null && !state.busy && !state.saving) { state.listing?.parent?.let(open) }
                IconButton("Enter folder path", "pencil", enabled = !state.saving) { state.pathInput = state.listing?.path.orEmpty(); state.pathOpen = !state.pathOpen }
            }
            AnimatedVisibility(state.pathOpen) {
                Row(Modifier.background(Palette.surface, RoundedCornerShape(10.dp)).padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
                    PlainTextField(state.pathInput, { state.pathInput = it }, Modifier.weight(1f), prompt = "Paste an absolute path", singleLine = true, enabled = !state.saving)
                    IconButton("Go to folder", "arrow.right", enabled = state.pathInput.isNotBlank() && !state.saving) { open(state.pathInput.trim()) }
                }
            }
            if (!noRecent) {
                SearchField(state.filter, { state.filter = it }, prompt = if (state.mode == FolderMode.Recent) "Search recent workspaces" else "Filter this folder")
                Text(tr(if (state.mode == FolderMode.Recent) "Recent workspaces" else "Folders"), style = Type.caption, color = Palette.secondary)
                when {
                    state.busy -> Box(Modifier.fillMaxWidth().padding(24.dp), contentAlignment = Alignment.Center) { Spinner(20.dp) }
                    state.mode == FolderMode.Recent -> {
                        val items = state.listing?.recent.orEmpty().filter { it.path.contains(state.filter, ignoreCase = true) }
                        Column {
                            items.forEach { item ->
                                Row(Modifier.fillMaxWidth().background(if (state.selected == item.path) Palette.surface else androidx.compose.ui.graphics.Color.Transparent, RoundedCornerShape(12.dp))
                                    .plainButton(enabled = !state.saving) { state.selected = item.path }.padding(12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                                    Symbol("folder", size = 18.dp, tint = Palette.secondary)
                                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                        Text(item.path.split('/', '\\').lastOrNull { it.isNotEmpty() } ?: item.path, style = Type.body, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                        Text(item.path, style = Type.caption, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    }
                                    if (state.selected == item.path) Symbol("checkmark", size = 16.dp)
                                }
                            }
                            if (items.isEmpty()) FolderEmpty(if (state.filter.isEmpty()) "Choose a folder to see it here next time." else "No matching folders.")
                        }
                    }
                    else -> {
                        val dirs = state.listing?.dirs.orEmpty().filter { it.name.contains(state.filter, ignoreCase = true) }
                        Column {
                            dirs.forEach { dir ->
                                Row(Modifier.fillMaxWidth().plainButton(enabled = !state.saving) { open(dir.path) }.padding(vertical = 13.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                                    Symbol(if (dir.isGit) "arrow.triangle.branch" else "folder", size = 18.dp, tint = Palette.secondary)
                                    Text(dir.name, Modifier.weight(1f), style = Type.body, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    Symbol("chevron.right", size = 12.dp, tint = Palette.tertiary)
                                }
                            }
                            if (state.listing != null && dirs.isEmpty()) FolderEmpty(if (state.filter.isEmpty()) "No subfolders. You can use this folder." else "No matching folders.")
                        }
                    }
                }
            }
            state.error?.let { Text(tr(it), style = Type.subheadline, color = Palette.danger) }
            if (state.mode == FolderMode.Recent && !noRecent) FolderBrowse(!state.saving) { open(start) }
            Spacer(Modifier.height(12.dp))
        }
        Row(Modifier.fillMaxWidth().background(Palette.surface).padding(20.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(tr(if (state.mode == FolderMode.Recent) "Selected folder" else "Current folder"), style = Type.caption, color = Palette.secondary)
                Text(state.selected.ifEmpty { tr("Choose a folder") }, style = Type.caption, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            PrimaryButton(if (state.saving) "Saving…" else "Use this folder", enabled = state.selected.isNotEmpty() && online && !state.busy && !state.saving) {
                scope.launch { state.confirm(load, select)?.let(pick) }
            }
        }
    }
}

@Composable private fun FolderEmpty(message: String) {
    Box(Modifier.fillMaxWidth().padding(vertical = 24.dp), contentAlignment = Alignment.Center) {
        Text(tr(message), style = Type.subheadline, color = Palette.secondary)
    }
}

@Composable private fun FolderBrowse(enabled: Boolean, onBrowse: () -> Unit) {
    Row(Modifier.fillMaxWidth().plainButton(enabled = enabled, action = onBrowse).padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Symbol("folder", size = 18.dp)
        Text(tr("Browse other folders"), Modifier.weight(1f), style = Type.body)
        Symbol("chevron.right", size = 12.dp)
    }
}
