package com.codync.android

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import com.codync.android.core.Entry
import com.codync.android.design.MenuItem
import com.codync.android.design.ReactionPick

/**
 * What a message's long-press menu holds past its copy items: the reactions on top, then Reply and Show what it did
 * (each when the row offers it). A message's text copies with [textCopyItem]; a picture in it with [imageCopyItems], in
 * the same place.
 */
internal class MessageMenu(val reactions: ReactionPick, val reply: (() -> Unit)?, val trace: (() -> Unit)?) {
    /** `copy` first, then the rest; `leave` runs before an item that goes elsewhere (the viewer closes itself). */
    fun items(copy: List<MenuItem>, leave: () -> Unit = {}): List<MenuItem> = copy + listOfNotNull(
        reply?.let { MenuItem("Reply", icon = "arrowshape.turn.up.left") { leave(); it() } },
        trace?.let { MenuItem("Show what it did", icon = "list.bullet") { leave(); it() } })

    companion object {
        fun of(entry: Entry, store: AppStore, reply: (() -> Unit)?, trace: (() -> Unit)?) = MessageMenu(reactionPick(entry, store), reply, trace)
    }
}

/** Copy, for a message's text. */
internal fun textCopyItem(context: Context, text: String) = MenuItem("Copy", icon = "square.on.square") {
    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Message", text))
}

/** What replaces a message's Copy when it holds a picture (desktop `imageCopyItems`): the picture itself, or just its name. */
internal fun imageCopyItems(copyImage: () -> Unit, copyName: () -> Unit) = listOf(
    MenuItem("Copy image", icon = "photo.on.rectangle", action = copyImage),
    MenuItem("Copy name", icon = "square.on.square", action = copyName))

/** [imageCopyItems] for `source` in a chat or the Library: a failed copy shows in the app's error dialog. */
internal fun imageCopyItems(store: AppStore, source: ImageSource) =
    imageCopyItems({ store.copyImageLater(source) }) { store.copyImageName(source, ImageFrom.Menu) }

internal val quickReactions = listOf("👍", "❤️", "😂", "🎉", "👀", "✅")

private fun reactionPick(entry: Entry, store: AppStore) = ReactionPick(quickReactions, entry.reactions) { emoji ->
    store.command("react", body("entryId" to entry.id, "emoji" to emoji), "react:" + entry.id)
}
