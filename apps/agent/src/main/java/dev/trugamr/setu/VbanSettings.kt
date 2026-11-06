package dev.trugamr.setu

import dev.trugamr.setu.protocol.StreamProtocol
import dev.trugamr.setu.protocol.Vban
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.UnknownHostException

/** The VBAN settings as the user typed them; [validate] turns them into a target. */
data class VbanSettings(
    val enabled: Boolean = false,
    /** The receiving PC's IP address. */
    val host: String = "",
    val port: String = Vban.PORT.toString(),
    val streamName: String = DEFAULT_STREAM_NAME,
) {
    sealed interface Validated {
        data object Off : Validated

        data class Send(val target: StreamProtocol.VbanTarget) : Validated

        /** Enabled, but a field is not usable yet; [message] says which and why. */
        data class Invalid(val message: String) : Validated
    }

    fun validate(): Validated {
        if (!enabled) return Validated.Off
        val address = ipAddress(host.trim()) ?: return Validated.Invalid("Enter the PC's IP address, like 192.168.1.20.")
        val port = port.trim().toIntOrNull()?.takeIf { it in 1..65535 }
            ?: return Validated.Invalid("The port is a number from 1 to 65535.")
        val name = streamName.trim()
        return try {
            Validated.Send(StreamProtocol.VbanTarget(InetSocketAddress(address, port), name))
        } catch (e: IllegalArgumentException) {
            Validated.Invalid("The stream name is 1 to 16 plain letters, digits or symbols.")
        }
    }

    companion object {
        const val DEFAULT_STREAM_NAME = "setu"

        private val IPV4 = Regex("""(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}""")

        /** An IPv4 or IPv6 literal, parsed without a name lookup; null for anything else. */
        private fun ipAddress(text: String): InetAddress? {
            if (!IPV4.matches(text) && !text.contains(':')) return null
            return try {
                InetAddress.getByName(text)
            } catch (e: UnknownHostException) {
                null
            }
        }
    }
}
