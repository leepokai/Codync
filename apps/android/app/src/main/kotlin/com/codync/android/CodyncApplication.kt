package com.codync.android

import android.app.Application
import com.codync.android.core.LocalCloud

/** Runs first in every process (activities, push, widgets): a debug build may use a cloud on the developer's machine. */
class CodyncApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        LocalCloud.allowed = BuildConfig.DEBUG
    }
}
