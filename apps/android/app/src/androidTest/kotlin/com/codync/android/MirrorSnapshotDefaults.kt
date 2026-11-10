package com.codync.android

/** Tests that read the whole mirror. Production [MirrorDatabase.snapshot] has no default and never loads every row on its own. */
internal fun MirrorDatabase.snapshot() = snapshot(Residency.All)
