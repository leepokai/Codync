package com.codync.android

import android.app.Activity
import android.content.Intent
import android.os.Bundle

/**
 * Where another app's share sheet sends text (the iPhone's Send Text to Bot). The sheet starts it in the sender's task,
 * so it hands the text on to Codync's own task and its one MainActivity (and account), then closes; `ShareToBot` picks the bot.
 */
class ShareActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Styled text arrives as a CharSequence, which getStringExtra drops.
        val text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
        if (!text.isNullOrBlank()) startActivity(Intent(this, MainActivity::class.java).setAction(Intent.ACTION_SEND)
            .putExtra(Intent.EXTRA_TEXT, text).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP))
        finish()
    }
}
