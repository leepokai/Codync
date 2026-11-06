package com.codync.android

import android.os.Build
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeTrue

/** Mutating live suites require an emulator and the controller's disposable host ID. */
internal fun requireFixtureHost(computerId: String) {
    assumeTrue(Build.PRODUCT.startsWith("sdk"))
    val expected = requireNotNull(InstrumentationRegistry.getArguments().getString("fixtureComputerId")) {
        "Pass fixtureComputerId for the isolated acceptance host. Never run these suites on a user phone."
    }
    assertEquals("This suite must use its disposable host", expected, computerId)
}
