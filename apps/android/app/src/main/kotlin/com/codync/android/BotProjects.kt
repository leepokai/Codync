package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.Text
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
import com.codync.android.design.*

/**
 * A host with projects (`hello.projects`): the bot's projects, the default first, with the worktree mode once one is a
 * git repository, then its own folder, where it always lives.
 */
internal fun CardRows.projectRows(bot: Bot?, draft: BotDraft, change: (BotDraft) -> Unit, addProject: () -> Unit) {
    val projects = draft.projects.orEmpty()
    row {
        ValueRow("Projects", detail = "Threads start in the default project. The bot can work elsewhere when you ask.") {
            IconButton("Add project", "plus", action = addProject)
        }
    }
    projects.forEachIndexed { index, path ->
        row { ProjectRow(path, index == 0, bot?.isGitProject(path) == true, { change(draft.withDefaultProject(path)) }) { change(draft.withoutProject(path)) } }
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
@Composable private fun ProjectRow(path: String, isDefault: Boolean, git: Boolean, makeDefault: () -> Unit, remove: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Symbol(if (git) "arrow.triangle.branch" else "folder", size = 16.dp, tint = Palette.secondary)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(folderName(path), style = Type.body, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(path, style = Type.caption, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (isDefault) Text(tr("Default"), style = Type.caption, color = Palette.secondary)
        else IconButton("Make default", "star", action = makeDefault)
        IconButton("Remove project", "minus.circle", action = remove)
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
