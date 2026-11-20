package com.codync.android

import android.app.Activity
import android.os.Bundle

/**
 * Where another app's share sheet sends text, links and files (the iPhone's Share extension). The sheet starts it in the
 * sender's task, so it hands them on to Codync's own task and its one MainActivity (and account), then closes; `ShareToBot`
 * picks the bot. The files' read grant goes along with them ([SharedContent.forward]).
 */
class ShareActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val shared = SharedContent.from(intent, packageName)
        if (!shared.isEmpty) startActivity(shared.forward(this))
        finish()
    }
}
