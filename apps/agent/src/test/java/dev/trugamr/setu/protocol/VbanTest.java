package dev.trugamr.setu.protocol;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertThrows;

import java.util.Arrays;

import org.junit.Test;

public class VbanTest {

    private static final StreamProtocol.Format FORMAT = new StreamProtocol.Format(48000, 2, 2, 96);

    @Test
    public void audio() throws Exception {
        byte[] fixture = Fixtures.read("vban-audio");
        byte[] pcm = {0x01, 0x00, (byte) 0xff, (byte) 0xff, 0x00, 0x01, 0x00, (byte) 0xff};
        Vban.Encoder encoder = new Vban.Encoder(FORMAT, "Quest");
        byte[] packet = encoder.newPacket();
        int length = encoder.audio(packet, 7, pcm, 0, pcm.length);
        assertArrayEquals(fixture, Arrays.copyOf(packet, length));
    }

    @Test
    public void packetsStayWithinTheProtocolLimits() {
        // 256 frames per packet; 16-bit stereo frames are 4 bytes, so 1024 bytes of data.
        Vban.Encoder stereo = new Vban.Encoder(FORMAT, "Quest");
        assertEquals(256, stereo.maxFrames());
        assertEquals(28 + 1024, stereo.newPacket().length);
        byte[] full = new byte[256 * 4];
        assertEquals(28 + 1024, stereo.audio(stereo.newPacket(), 0, full, 0, full.length));
        assertThrows(IllegalArgumentException.class,
                () -> stereo.audio(stereo.newPacket(), 0, new byte[257 * 4], 0, 257 * 4));
        // 8 channels: 1436 data bytes hold 89 frames.
        assertEquals(89, new Vban.Encoder(new StreamProtocol.Format(48000, 8, 2, 96), "Quest").maxFrames());
    }

    @Test
    public void streamNamesAreShortAscii() {
        assertEquals("Sixteen chars ok", Vban.checkStreamName("Sixteen chars ok"));
        assertThrows(IllegalArgumentException.class, () -> Vban.checkStreamName(""));
        assertThrows(IllegalArgumentException.class, () -> Vban.checkStreamName("Seventeen chars!!"));
        assertThrows(IllegalArgumentException.class, () -> Vban.checkStreamName("Quêst"));
    }

    @Test
    public void formatsVbanCannotCarryAreRejected() {
        assertThrows(IllegalArgumentException.class,
                () -> new Vban.Encoder(new StreamProtocol.Format(47000, 2, 2, 96), "Quest"));
        assertThrows(IllegalArgumentException.class,
                () -> new Vban.Encoder(new StreamProtocol.Format(48000, 2, 3, 96), "Quest"));
    }
}
