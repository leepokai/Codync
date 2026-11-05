package com.codync.android.core

import org.junit.Assert.assertEquals
import org.junit.Test

class FillTest {
    @Test fun `what it puts in stays as written`() {
        assertEquals("Build {other} \$1 and Reviewer", fill("{name} and {other}", "name" to "Build {other} \$1", "other" to "Reviewer"))
        assertEquals("Build and {missing}", fill("{name} and {missing}", "name" to "Build"))
    }
}
