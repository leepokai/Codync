package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.BotDraft
import com.codync.android.core.WorktreeMode
import com.codync.android.core.folderName
import com.codync.android.core.sharedFolderName
import com.codync.android.design.*

/**
 * A host with projects (`hello.projects`): the bot's projects, the default first, with the worktree mode once one is a
 * git repository, then its own folder, where it always lives.
 */
internal fun CardRows.projectRows(bot: Bot?, draft: BotDraft, worktreeFill: Boolean, change: (BotDraft) -> Unit, addProject: () -> Unit) {
    val projects = draft.projects.orEmpty()
    row {
        ValueRow("Projects", detail = "Threads start in the default project. The bot can work elsewhere when you ask.") {
            IconButton("Add project", "plus", action = addProject)
        }
    }
    projects.forEachIndexed { index, path ->
        val git = bot?.isGitProject(path) == true
        row {
            ProjectRow(path, index == 0, git, { change(draft.withDefaultProject(path)) }, { change(draft.withoutProject(path)) }) {
                // Only where a worktree is made (a git project) and the host fills it.
                if (git && worktreeFill && draft.sharedFolders != null) SharedFoldersButton(path, draft, change)
            }
        }
    }
    if (bot != null && projects.any(bot::isGitProject)) row {
        val mode = draft.worktreeMode ?: WorktreeMode.SideBySide
        ValueRow("Worktree mode", detail = mode.detail) {
            DropdownMenu({ WorktreeMode.entries.map { MenuItem(it.title, selected = it == mode, detail = it.detail) { change(draft.copy(worktreeMode = it)) } } },
                label = "${tr("Worktree mode")}: ${tr(mode.title)}") {
                Row(Modifier.outline().pill(Color.Transparent), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(tr(mode.title), style = Type.body, color = Palette.text, maxLines = 1)
                    Symbol("chevron.down", size = 13.dp, tint = Palette.secondary)
                }
            }
        }
    }
    // A new bot has no folder until it's created.
    bot?.cwd?.takeIf(String::isNotEmpty)?.let { home -> row { HomeRow(home) } }
}

/** One project: its folder's name over its path, "Default" on the first, Make default on the others, and Remove. */
@Composable private fun ProjectRow(path: String, isDefault: Boolean, git: Boolean, makeDefault: () -> Unit, remove: () -> Unit,
    actions: @Composable () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Symbol(if (git) "arrow.triangle.branch" else "folder", size = 16.dp, tint = Palette.secondary)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(folderName(path), style = Type.body, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(path, style = Type.caption, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        actions()
        if (isDefault) Text(tr("Default"), style = Type.caption, color = Palette.secondary)
        else IconButton("Make default", "star", action = makeDefault)
        IconButton("Remove project", "minus.circle", action = remove)
    }
}

/** Shared folders of [project]: the button on its row, and the sheet that lists them, each with Remove, and adds one. */
@Composable private fun SharedFoldersButton(project: String, draft: BotDraft, change: (BotDraft) -> Unit) {
    var open by remember { mutableStateOf(false) }
    IconButton("Shared folders", "folder") { open = true }
    CodyncSheet(open, { open = false }, fitted = true) {
        var name by remember { mutableStateOf("") }
        val add = { change(draft.withSharedFolder(project, name)); name = "" }
        Column(Modifier.fillMaxWidth().padding(bottom = 20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            ModalHeader("Shared folders")
            Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(tr("Linked into each new worktree, so installed packages don't have to be installed again."), style = Type.caption,
                    color = Palette.secondary)
                draft.sharedFolders(project).forEach { folder ->
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Text(folder, Modifier.weight(1f), style = Type.body.monospaced, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        IconButton("Remove folder", "minus.circle") { change(draft.withoutSharedFolder(project, folder)) }
                    }
                }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Box(Modifier.weight(1f)) {
                        FieldBox(name, { name = it }, "Folder name", style = Type.body.monospaced, capitalize = false, imeAction = ImeAction.Done,
                            actions = KeyboardActions(onDone = { add() }))
                    }
                    IconButton("Add folder", "plus", enabled = sharedFolderName(name) != null) { add() }
                }
            }
        }
    }
}

/** The bot's own folder, where it always lives: shown as it is, a tap copies it. */
@Composable private fun HomeRow(home: String) {
    val (copied, copy) = rememberCopyPath()
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(tr("Bot's folder"), style = Type.body)
            Text(tr("Its chat, routines and files live here."), style = Type.caption, color = Palette.secondary)
            Text(if (copied) tr("Path copied") else home, style = Type.caption, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        IconButton("Copy path", "doc.on.doc") { copy(home) }
    }
}
