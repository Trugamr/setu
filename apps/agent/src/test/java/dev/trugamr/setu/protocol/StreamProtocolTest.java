package dev.trugamr.setu.protocol;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertThrows;

import java.net.InetSocketAddress;
import java.util.Collections;

import org.junit.Test;

public class StreamProtocolTest {

    private static final StreamProtocol.VbanTarget QUEST =
            new StreamProtocol.VbanTarget(new InetSocketAddress("192.168.1.20", 6980), "Quest");

    @Test
    public void hello() throws Exception {
        byte[] fixture = Fixtures.read("hello");
        assertArrayEquals(fixture, StreamProtocol.hello("studio-pc"));
        assertEquals(StreamProtocol.HELLO, StreamProtocol.type(fixture, fixture.length));
        assertEquals("studio-pc", StreamProtocol.parseHello(fixture, fixture.length));
    }

    @Test
    public void format() throws Exception {
        byte[] fixture = Fixtures.read("format");
        StreamProtocol.Format format = new StreamProtocol.Format(48000, 2, 2, 96);
        assertArrayEquals(fixture, StreamProtocol.format(format));
        assertEquals(format, StreamProtocol.parseFormat(fixture, fixture.length));
    }

    @Test
    public void audio() throws Exception {
        byte[] fixture = Fixtures.read("audio");
        byte[] pcm = {0x01, 0x00, (byte) 0xff, (byte) 0xff, 0x00, 0x01, 0x00, (byte) 0xff};
        byte[] packet = new byte[64];
        int length = StreamProtocol.audio(packet, 7, 1000, pcm, 0, pcm.length);
        assertEquals(fixture.length, length);
        assertArrayEquals(fixture, java.util.Arrays.copyOf(packet, length));
        assertEquals(7, StreamProtocol.audioSequence(fixture, fixture.length));
        assertEquals(1000, StreamProtocol.audioFramePosition(fixture, fixture.length));
    }

    @Test
    public void status() throws Exception {
        byte[] fixture = Fixtures.read("status");
        StreamProtocol.Status status = new StreamProtocol.Status(96,
                Collections.singletonList(new StreamProtocol.Receiver("studio-pc", "192.168.1.20:50000")), QUEST);
        assertArrayEquals(fixture, StreamProtocol.status(status));
        assertEquals(status, StreamProtocol.parseStatus(fixture, fixture.length));
    }

    @Test
    public void setVban() throws Exception {
        byte[] fixture = Fixtures.read("set-vban");
        assertArrayEquals(fixture, StreamProtocol.setVban(QUEST));
        assertEquals(StreamProtocol.SET_VBAN, StreamProtocol.type(fixture, fixture.length));
        assertEquals(QUEST, StreamProtocol.parseSetVban(fixture, fixture.length));

        byte[] off = Fixtures.read("set-vban-off");
        assertArrayEquals(off, StreamProtocol.setVban(null));
        assertEquals(null, StreamProtocol.parseSetVban(off, off.length));
    }

    @Test
    public void vbanTargetsAreIpAddresses() throws Exception {
        StreamProtocol.VbanTarget v6 = new StreamProtocol.VbanTarget(new InetSocketAddress("fe80::1", 6980), "Quest");
        byte[] message = StreamProtocol.setVban(v6);
        assertEquals(v6, StreamProtocol.parseSetVban(message, message.length));

        byte[] badLength = {'S', 'T', 1, StreamProtocol.SET_VBAN, 5, 1, 2, 3, 4, 5};
        assertThrows(IllegalArgumentException.class, () -> StreamProtocol.parseSetVban(badLength, badLength.length));
        assertThrows(IllegalArgumentException.class,
                () -> new StreamProtocol.VbanTarget(InetSocketAddress.createUnresolved("studio-pc", 6980), "Quest"));
    }

    @Test
    public void emptyMessages() throws Exception {
        assertArrayEquals(Fixtures.read("status-request"), StreamProtocol.empty(StreamProtocol.STATUS_REQUEST));
        assertArrayEquals(Fixtures.read("stop"), StreamProtocol.empty(StreamProtocol.STOP));
    }

    @Test
    public void foreignDatagramsHaveNoType() {
        assertEquals(-1, StreamProtocol.type(new byte[] {'Q', 'A', 'H', 'I'}, 4));
        assertEquals(-1, StreamProtocol.type(new byte[] {'S', 'T', 2, 1}, 4));
        assertEquals(-1, StreamProtocol.type(new byte[] {'S', 'T'}, 2));
    }

    @Test
    public void truncatedMessagesAreRejected() throws Exception {
        byte[] status = Fixtures.read("status");
        assertThrows(IllegalArgumentException.class, () -> StreamProtocol.parseStatus(status, status.length - 1));
        byte[] format = Fixtures.read("format");
        assertThrows(IllegalArgumentException.class, () -> StreamProtocol.parseFormat(format, format.length - 1));
        byte[] setVban = Fixtures.read("set-vban");
        assertThrows(IllegalArgumentException.class, () -> StreamProtocol.parseSetVban(setVban, 6));
    }

    @Test
    public void wrongTypeIsRejected() throws Exception {
        byte[] status = Fixtures.read("status");
        assertThrows(IllegalArgumentException.class, () -> StreamProtocol.parseFormat(status, status.length));
    }
}
