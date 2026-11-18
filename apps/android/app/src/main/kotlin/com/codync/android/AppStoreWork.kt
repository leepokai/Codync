package com.codync.android

// The user's hand on a bot's box: order, cutting in, stopping, dropping and trying again. The host decides what runs.

/** A waiting item moves to the front. */
internal fun AppStore.workFirst(botId: String, itemId: String) = changeWork("workFirst", botId, itemId)

/** A waiting item, or one that ended, leaves the box. */
internal fun AppStore.dropWork(botId: String, itemId: String) = changeWork("dropWork", botId, itemId)

/** A working item's task stops. */
internal fun AppStore.stopWork(botId: String, itemId: String) = changeWork("stopWork", botId, itemId)

/** A stopped or failed item goes back to waiting, last. */
internal fun AppStore.retryWork(botId: String, itemId: String) = changeWork("retryWork", botId, itemId)

/** Starts [itemId] now in the place of the working item [insteadOf], which returns to the front and starts again later. */
internal fun AppStore.workCutIn(botId: String, itemId: String, insteadOf: String) =
    command("workCutIn", body("botId" to botId, "itemId" to itemId, "insteadOf" to insteadOf), workKey(botId))

private fun AppStore.changeWork(method: String, botId: String, itemId: String) =
    command(method, body("botId" to botId, "itemId" to itemId), workKey(botId))

/** Busy while one of [botId]'s work changes is out. */
internal fun workKey(botId: String) = "work:$botId"
