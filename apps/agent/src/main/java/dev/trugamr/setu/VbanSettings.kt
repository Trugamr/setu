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
        val problem = hostProblem(host) ?: portProblem(port) ?: streamNameProblem(streamName)
        if (problem != null) return Validated.Invalid(problem)
        val address = InetSocketAddress(parseIpLiteral(host.trim()), port.trim().toInt())
        return Validated.Send(StreamProtocol.VbanTarget(address, streamName.trim()))
    }

    companion object {
        /** Why [host] can't be the PC's address, or null when it can. */
        fun hostProblem(host: String): String? {
            val address = parseIpLiteral(host.trim()) ?: return "Enter the PC's IP address, like 192.168.1.20."
            if (address is Inet6Address && address.isLinkLocalAddress) {
                return "Use the PC's IPv4 address. A link-local IPv6 address (fe80::) does not work here."
            }
            return null
        }

        /** Why [port] can't be the receiver's port, or null when it can. */
        fun portProblem(port: String): String? =
            if (port.trim().toIntOrNull() in 1..65535) null else "The port is a number from 1 to 65535."

        /** Why VBAN can't carry [name] as a stream name, or null when it can. */
        fun streamNameProblem(name: String): String? = try {
            Vban.checkStreamName(name.trim())
            null
        } catch (e: IllegalArgumentException) {
            "The stream name is 1 to ${Vban.MAX_STREAM_NAME_LENGTH} plain letters, digits or symbols."
        }

        /** Settings that send to [target]. */
        fun of(target: StreamProtocol.VbanTarget) = VbanSettings(
            enabled = true,
            host = target.address.address.hostAddress.orEmpty(),
            port = target.address.port.toString(),
            streamName = target.streamName,
        )
    }
}
