package com.codync.android

import java.net.URLEncoder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PairingCopyTest {
    @Test fun `a pasted private relay is named and a bad link stays on Cloudflare`() {
        val cloudflare = "Connects on Wi-Fi, over Tailscale, or from anywhere through Cloudflare, on its own. End-to-end encrypted."
        assertEquals(cloudflare, pairingLine("not a link", "codync"))
        val link = "codync://pair?" + mapOf(
            "v" to "3",
            "id" to "NHUPmL1Z_PyUbaRaqr6TOw",
            "sk" to "iojj3XQJ8ZX9UtstPLpdcspnCb8dlBIb83SIAbQPb1w",
            "bk" to "zo060cy2M-x7cMF4FKXHbs0CloUFDTRHRboFhw5YfVk",
            "code" to "CAgICAgICAgICAgICAgICA",
            "cloud" to "https://relay.example.com",
        ).entries.joinToString("&") { "${it.key}=${URLEncoder.encode(it.value, "UTF-8")}" }
        assertTrue(pairingLine(link, "codync").contains("Private relay (relay.example.com)"))
    }
}
