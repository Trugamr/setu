package dev.trugamr.setu

import dev.trugamr.setu.protocol.StreamProtocol
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress
import kotlin.concurrent.thread
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class LoopbackServerTest {

    @Test
    fun asksForStatusAndSendsSettings() {
        val fake = DatagramSocket(0, InetAddress.getLoopbackAddress())
        val quest = StreamProtocol.VbanTarget(InetSocketAddress("192.168.1.20", 6980), "setu")
        val status = StreamProtocol.Status(96, emptyList(), quest)
        val received = mutableListOf<Int>()
        val serving = thread {
            fake.use {
                repeat(2) {
                    val buffer = ByteArray(2048)
                    val packet = DatagramPacket(buffer, buffer.size)
                    fake.receive(packet)
                    val type = StreamProtocol.type(buffer, packet.length)
                    received += type
                    if (type == StreamProtocol.STATUS_REQUEST) {
                        val reply = StreamProtocol.status(status)
                        fake.send(DatagramPacket(reply, reply.size, packet.socketAddress))
                    } else {
                        assertEquals(quest, StreamProtocol.parseSetVban(buffer, packet.length))
                    }
                }
            }
        }
        val server = LoopbackServer(fake.localPort, timeoutMs = 2000)
        assertEquals(status, server.status())
        server.setVban(quest)
        serving.join(3000)
        assertEquals(listOf(StreamProtocol.STATUS_REQUEST, StreamProtocol.SET_VBAN), received)
    }

    @Test
    fun noServerMeansNoStatus() {
        val port = DatagramSocket(0, InetAddress.getLoopbackAddress()).use { it.localPort }
        assertNull(LoopbackServer(port, timeoutMs = 200).status())
    }
}
