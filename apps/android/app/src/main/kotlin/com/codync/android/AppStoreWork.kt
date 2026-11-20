package com.codync.android

// The user's hand on a bot's box: order, cutting in, stopping, dropping and trying again. The host decides what runs.

/** A waiting task moves to the front. */
internal fun AppStore.workFirst(botId: String, taskId: String) = changeWork("workFirst", botId, taskId)

/** A waiting task, or one that ended, leaves the box. */
internal fun AppStore.dropWork(botId: String, taskId: String) = changeWork("dropWork", botId, taskId)

/** A working task's thread stops. */
internal fun AppStore.stopWork(botId: String, taskId: String) = changeWork("stopWork", botId, taskId)

/** A stopped or failed task goes back to waiting, last. */
internal fun AppStore.retryWork(botId: String, taskId: String) = changeWork("retryWork", botId, taskId)

/** Starts [taskId] now in the place of the working task [insteadOf], which returns to the front and starts again later. */
internal fun AppStore.workCutIn(botId: String, taskId: String, insteadOf: String) =
    command("workCutIn", body("botId" to botId, "taskId" to taskId, "insteadOf" to insteadOf), workKey(botId))

private fun AppStore.changeWork(method: String, botId: String, taskId: String) =
    command(method, body("botId" to botId, "taskId" to taskId), workKey(botId))

/** Busy while one of [botId]'s work changes is out. */
internal fun workKey(botId: String) = "work:$botId"
