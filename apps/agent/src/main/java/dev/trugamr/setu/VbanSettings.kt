package dev.trugamr.setu

import dev.trugamr.setu.protocol.StreamProtocol
import dev.trugamr.setu.protocol.Vban
import java.net.Inet6Address
import java.net.InetSocketAddress

/** The VBAN settings as the user typed them; [validate] turns them into a target. */
data class VbanSettings(
    val enabled: Boolean = false,
    /** The receiving PC's IP address. */
    val host: String = "",
    val port: String = Vban.PORT.toString(),
    val streamName: String = Vban.DEFAULT_STREAM_NAME,
) {
    sealed interface Validated {
        data object Off : Validated

        data class Send(val target: StreamProtocol.VbanTarget) : Validated

        /** Enabled, but a field is not usable yet; [message] says which and why. */
        data class Invalid(val message: String) : Validated
    }

    fun validate(): Validated {
        if (!enabled) return Validated.Off
        val address = parseIpLiteral(host.trim())
            ?: return Validated.Invalid("Enter the PC's IP address, like 192.168.1.20.")
        if (address is Inet6Address && address.isLinkLocalAddress) {
            return Validated.Invalid("Use the PC's IPv4 address. A link-local IPv6 address (fe80::) does not work here.")
        }
        val port = port.trim().toIntOrNull()?.takeIf { it in 1..65535 }
            ?: return Validated.Invalid("The port is a number from 1 to 65535.")
        val name = streamName.trim()
        return try {
            Validated.Send(StreamProtocol.VbanTarget(InetSocketAddress(address, port), name))
        } catch (e: IllegalArgumentException) {
            Validated.Invalid("The stream name is 1 to ${Vban.MAX_STREAM_NAME_LENGTH} plain letters, digits or symbols.")
        }
    }

    companion object {
        /** Settings that send to [target]. */
        fun of(target: StreamProtocol.VbanTarget) = VbanSettings(
            enabled = true,
            host = target.address.address.hostAddress.orEmpty(),
            port = target.address.port.toString(),
            streamName = target.streamName,
        )
    }
}
