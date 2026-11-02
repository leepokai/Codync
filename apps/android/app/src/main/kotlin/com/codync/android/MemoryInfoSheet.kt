package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.unit.dp
import com.codync.android.design.*

// The memory screen's info buttons (iOS `MemoryInfoSheet`): what the harness's own memory means for the bot, and what
// dreaming does. The same text as the iPhone's and the desktop app's.

internal class InfoPoint(val icon: String, val title: String, val detail: String)

internal class InfoSheetContent(val title: String, val points: List<InfoPoint>)

/** How the bot reaches its harness's own memory. */
internal enum class HarnessAccess {
    /** A switch, and Codync shows the notes (Claude Code's are a file on the computer). */
    Readable,
    /** A switch only. */
    Hidden,
    /** No switch: one memory every bot on the harness shares (Hermes Agent, Gemini CLI). */
    Shared,
}

/** The harness's name goes in after the sentence is translated, so it stays as written. */
@Composable internal fun harnessInfo(harness: String, access: HarnessAccess): InfoSheetContent {
    val translate = translator()
    val fill = { template: String -> translate(template).replace("{name}", harness) }
    val shared = access == HarnessAccess.Shared
    return InfoSheetContent(fill("{name}'s own memory"), listOfNotNull(
        InfoPoint("brain", "A memory of its own", fill(if (shared) "{name} keeps notes apart from Codync's memory, and Codync can't turn them off for one bot."
            else "{name} keeps notes apart from Codync's memory. It's off by default, so this bot has one memory and you see all of it here.")),
        if (shared) InfoPoint("person.2", "Shared by every bot", fill("Every bot that runs {name} reads and writes the same notes, and so does {name} when you use it outside Codync.")) else null,
        InfoPoint("square.on.square", "Remembered twice", "With both on, the bot may save the same thing twice, and the two can disagree after something changes."),
        InfoPoint("trash", "Forgetting", fill("What the bot forgets in Codync stays in {name}'s notes. Codync can't change them.")),
        InfoPoint("eye", "Where to see it", fill(if (access == HarnessAccess.Readable) "Its notes show read-only under the {name} tab." else "Codync can't show it; look in {name} itself.")),
        if (shared) InfoPoint("gearshape", "Turning it off", fill("Where {name} has a setting for it, that setting turns it off for every session at once.")) else null,
    ))
}

internal val DREAMS_INFO = InfoSheetContent("Dreaming", listOf(
    InfoPoint("moon", "Tidying between chats", "After five quiet minutes or eight messages, or once memory is nearly full, the bot reads what you said since its last run and updates its memory."),
    InfoPoint("plus.circle", "Adds and takes out", "Green lines are memories it added. Red lines are ones it took out because they went out of date or moved into a note. It can also add a line to a note."),
    InfoPoint("text.bubble", "Only your words", "What it adds quotes something you typed yourself; pasted text, the bot's replies and other bots don't count. It never rewrites what you wrote. It may also put a passed date in the past tense, and before memory fills up it moves detail, word for word, into notes by topic, keeping one line per topic that points to its note."),
    InfoPoint("lock.shield", "Kept apart", "It runs in its own session, away from the chat, and fixed checks decide what it may save."),
    InfoPoint("eye", "A record", "This tab only shows what it did. To change a memory, tell the bot."),
))

@Composable internal fun InfoSheet(content: InfoSheetContent) {
    Column {
        ModalHeader(content.title)
        CardForm {
            CardSection {
                for (point in content.points) row {
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Symbol(point.icon, Modifier.padding(top = 2.dp), size = 20.dp, tint = Palette.secondary)
                        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(tr(point.title), style = Type.body, color = Palette.text)
                            Text(tr(point.detail), style = Type.footnote, color = Palette.secondary)
                        }
                    }
                }
            }
        }
    }
}
