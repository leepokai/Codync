package com.codync.android

import android.os.Build
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.lifecycle.ViewModelProvider
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

/** The account's ViewModel, the one [activity]'s UI uses. */
internal fun accountStore(activity: ComponentActivity): AccountStore = ViewModelProvider(activity)[AccountStore::class.java]

/** Computer [id]'s store (the first computer's when null), once the account has opened it. */
internal fun ComposeTestRule.computerStore(accounts: AccountStore, id: String? = null): AppStore {
    waitUntil(30_000) { accounts.stores.value.any { id == null || it.id == id } }
    return accounts.stores.value.first { id == null || it.id == id }
}

internal fun AndroidComposeTestRule<*, out ComponentActivity>.computerStore(id: String? = null): AppStore = computerStore(accountStore(activity), id)
