package dev.trugamr.setu

import java.net.InetAddress
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class IpLiteralTest {

    @Test
    fun parsesLiterals() {
        for ((text, expected) in listOf(
            "192.168.1.20" to "192.168.1.20",
            "0.0.0.0" to "0.0.0.0",
            "fe80::1" to "fe80::1",
            "::1" to "::1",
            "::" to "::",
            "2001:DB8::" to "2001:db8::",
            "1:2:3:4:5:6:7:8" to "1:2:3:4:5:6:7:8",
            "1::8" to "1::8",
            "64:ff9b::192.168.1.20" to "64:ff9b::c0a8:114",
            "::ffff:192.168.1.20" to "192.168.1.20",
        )) {
            // getByName parses a literal without a lookup, so it serves as the reference here.
            assertEquals(text, InetAddress.getByName(expected), parseIpLiteral(text))
        }
    }

    @Test
    fun rejectsNamesAndMalformedLiterals() {
        for (text in listOf(
            "", "studio-pc", "pc:1", "studio-pc:6980", "192.168.1.20:6980", "192.168.1", "192.168.1.256",
            "01.2.3.4", "1.2.3.4.", ":::", "1::2::3", "1:2:3:4:5:6:7", "1:2:3:4:5:6:7:8:9", "1:2:3:4:5:6:7::8",
            "12345::", "fe80::1%wlan0", "[::1]", "::1.2.3", "1.2.3.4::",
        )) {
            assertNull(text, parseIpLiteral(text))
        }
    }
}
