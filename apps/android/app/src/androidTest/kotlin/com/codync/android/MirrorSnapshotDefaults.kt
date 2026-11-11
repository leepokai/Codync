package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.WireJson

/** Tests that read the whole mirror. Production [MirrorDatabase.snapshot] has no default and never loads every row. */
internal fun MirrorDatabase.snapshot() = snapshot(Residency(emptySet(), preview = 0)).copy(entries =
    readableDatabase.rawQuery("SELECT payload FROM entries ORDER BY seq", null).use { cursor ->
        buildList { while (cursor.moveToNext()) add(WireJson.decodeFromString<Entry>(cursor.getString(0))) }
    })
