package dev.trugamr.setu

import dev.trugamr.setu.protocol.StreamProtocol
import java.io.IOException
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress

/**
 * The [CaptureServer] adapter: the server's UDP port on this headset. The server accepts status
 * requests and settings only from loopback, which is why the agent has to run on the headset.
 */
class LoopbackServer(
    private val serverPort: Int = StreamProtocol.UDP_PORT,
    private val timeoutMs: Int = 300,
) : CaptureServer {

    override fun status(): StreamProtocol.Status? = try {
        open().use { socket ->
            send(socket, StreamProtocol.empty(StreamProtocol.STATUS_REQUEST))
            val buffer = ByteArray(MAX_DATAGRAM_BYTES)
            val packet = DatagramPacket(buffer, buffer.size)
            socket.receive(packet)
            StreamProtocol.parseStatus(buffer, packet.length)
        }
    } catch (e: IOException) {
        // A timeout, or "port unreachable": no server is running.
        null
    } catch (e: IllegalArgumentException) {
        // Not a STATUS.
        null
    }

    override fun setVban(target: StreamProtocol.VbanTarget?) {
        try {
            open().use { send(it, StreamProtocol.setVban(target)) }
        } catch (e: IOException) {
            // No server; the next status shows it.
        }
    }

    override fun stop() {
        try {
            open().use { send(it, StreamProtocol.empty(StreamProtocol.STOP)) }
        } catch (e: IOException) {
            // No server, which is what STOP asks for.
        }
    }

    private fun open() = DatagramSocket().apply {
        soTimeout = timeoutMs
        connect(InetAddress.getLoopbackAddress(), serverPort)
    }

    private fun send(socket: DatagramSocket, message: ByteArray) {
        socket.send(DatagramPacket(message, message.size))
    }

    private companion object {
        /** The largest UDP payload, so a long receiver name cannot truncate a STATUS. */
        const val MAX_DATAGRAM_BYTES = 65_507
    }
}
