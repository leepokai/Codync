package com.codync.android

import com.codync.android.design.AppLanguage
import com.codync.android.design.translate
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

    @Test fun `the pairing check reads as two groups of three and fills the shared phrase`() {
        val line = pairingCheckLine("855066")
        assertEquals("If the computer asks, check it shows 855\u00A0066.", line)
        val table = mapOf("If the computer asks, check it shows {code}." to "如果電腦問你，確認它顯示 {code}。")
        assertEquals("如果電腦問你，確認它顯示 855\u00A0066。", translate(line, AppLanguage.ZhHant, systemIsChinese = false, table))
    }
}
