package dev.trugamr.setu

import dev.trugamr.setu.VbanSettings.Validated
import dev.trugamr.setu.protocol.StreamProtocol
import java.net.InetSocketAddress
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class VbanSettingsTest {

    @Test
    fun offWhenDisabledWhateverTheFields() {
        assertEquals(Validated.Off, VbanSettings(enabled = false, host = "not an address").validate())
    }

    @Test
    fun aCompleteTargetIsSent() {
        val settings = VbanSettings(enabled = true, host = " 192.168.1.20 ", port = "6981", streamName = "Stream1")
        val expected = StreamProtocol.VbanTarget(InetSocketAddress("192.168.1.20", 6981), "Stream1")
        assertEquals(Validated.Send(expected), settings.validate())
    }

    @Test
    fun hostsAreIpAddressesNotNames() {
        for (host in listOf("", "192.168.1", "192.168.1.256", "studio-pc", "192.168.1.20:6980")) {
            assertTrue(host, VbanSettings(enabled = true, host = host).validate() is Validated.Invalid)
        }
        assertTrue(VbanSettings(enabled = true, host = "fe80::1").validate() is Validated.Send)
    }

    @Test
    fun portAndStreamNameMustFitVban() {
        val base = VbanSettings(enabled = true, host = "192.168.1.20")
        for (bad in listOf(base.copy(port = "0"), base.copy(port = "70000"), base.copy(port = "x"),
            base.copy(streamName = ""), base.copy(streamName = "a name that is too long"))) {
            assertTrue(bad.toString(), bad.validate() is Validated.Invalid)
        }
    }
}
