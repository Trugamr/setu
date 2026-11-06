package dev.trugamr.setu.protocol;

import java.nio.charset.StandardCharsets;

/**
 * Encodes audio as VBAN, VB-Audio's protocol for audio over UDP (specification revision 13), so
 * that Voicemeeter and other VBAN receivers can play the stream. Pure byte work, like
 * {@link StreamProtocol}; the packet layout is in protocol/README.md, "VBAN".
 */
public final class Vban {

    /** The port VBAN receivers listen on by default. */
    public static final int PORT = 6980;
    /** The stream name Setu sends under unless told otherwise. */
    public static final String DEFAULT_STREAM_NAME = "setu";
    public static final int MAX_STREAM_NAME_LENGTH = 16;
    public static final int HEADER_BYTES = 28;
    /** Where the header keeps the stream name and the packet counter. */
    public static final int STREAM_NAME_OFFSET = 8;
    public static final int COUNTER_OFFSET = 24;

    /** The protocol's limits: frames per packet, and data bytes so a packet fits a 1500-byte MTU. */
    private static final int MAX_FRAMES = 256;
    private static final int MAX_DATA_BYTES = 1436;
    /** Sample rates by their index in the header. */
    private static final int[] SAMPLE_RATES = {
            6000, 12000, 24000, 48000, 96000, 192000, 384000,
            8000, 16000, 32000, 64000, 128000, 256000, 512000,
            11025, 22050, 44100, 88200, 176400, 352800, 705600,
    };
    /** Sample type 1, 16-bit integer, with codec 0, PCM. */
    private static final byte INT16_PCM = 0x01;

    private Vban() {
    }

    /**
     * Returns {@code name} if VBAN can carry it as a stream name: 1 to
     * {@link #MAX_STREAM_NAME_LENGTH} printable ASCII characters. Throws {@link IllegalArgumentException} otherwise.
     */
    public static String checkStreamName(String name) {
        if (name.isEmpty() || name.length() > MAX_STREAM_NAME_LENGTH) {
            throw new IllegalArgumentException("a VBAN stream name has 1 to " + MAX_STREAM_NAME_LENGTH + " characters: \"" + name + "\"");
        }
        for (int i = 0; i < name.length(); i++) {
            char c = name.charAt(i);
            if (c < 0x20 || c > 0x7e) {
                throw new IllegalArgumentException("a VBAN stream name is plain ASCII: \"" + name + "\"");
            }
        }
        return name;
    }

    /** Writes the packets of one stream: one format, one name. */
    public static final class Encoder {
        private final byte[] header = new byte[HEADER_BYTES];
        private final int frameBytes;
        private final int maxFrames;

        /** Throws {@link IllegalArgumentException} for a format or name VBAN cannot carry. */
        public Encoder(StreamProtocol.Format format, String streamName) {
            int rateIndex = sampleRateIndex(format.sampleRate);
            if (format.bytesPerSample != 2) {
                throw new IllegalArgumentException("only 16-bit PCM is encoded as VBAN, not " + format);
            }
            if (format.channels < 1 || format.channels > 256) {
                throw new IllegalArgumentException("VBAN carries 1 to 256 channels, not " + format.channels);
            }
            byte[] name = checkStreamName(streamName).getBytes(StandardCharsets.US_ASCII);
            header[0] = 'V';
            header[1] = 'B';
            header[2] = 'A';
            header[3] = 'N';
            header[4] = (byte) rateIndex;
            header[6] = (byte) (format.channels - 1);
            header[7] = INT16_PCM;
            System.arraycopy(name, 0, header, STREAM_NAME_OFFSET, name.length);
            frameBytes = format.frameBytes();
            maxFrames = Math.min(MAX_FRAMES, MAX_DATA_BYTES / frameBytes);
        }

        /** Frames one packet carries at most; a longer stretch of PCM is split. */
        public int maxFrames() {
            return maxFrames;
        }

        /** A buffer that holds any packet of this stream, for {@link #audio}. */
        public byte[] newPacket() {
            return new byte[HEADER_BYTES + maxFrames * frameBytes];
        }

        /**
         * Writes a packet into {@code packet} (from {@link #newPacket}): the header with
         * {@code counter}, then {@code pcmLength} bytes of {@code pcm} from {@code pcmOffset},
         * whole frames and at most {@link #maxFrames}. Returns the packet length.
         */
        public int audio(byte[] packet, int counter, byte[] pcm, int pcmOffset, int pcmLength) {
            int frames = pcmLength / frameBytes;
            if (frames < 1 || frames > maxFrames || pcmLength % frameBytes != 0) {
                throw new IllegalArgumentException("a VBAN packet holds 1 to " + maxFrames + " whole frames, not "
                        + pcmLength + " bytes");
            }
            System.arraycopy(header, 0, packet, 0, HEADER_BYTES);
            packet[5] = (byte) (frames - 1);
            packet[COUNTER_OFFSET] = (byte) counter;
            packet[COUNTER_OFFSET + 1] = (byte) (counter >>> 8);
            packet[COUNTER_OFFSET + 2] = (byte) (counter >>> 16);
            packet[COUNTER_OFFSET + 3] = (byte) (counter >>> 24);
            System.arraycopy(pcm, pcmOffset, packet, HEADER_BYTES, pcmLength);
            return HEADER_BYTES + pcmLength;
        }
    }

    private static int sampleRateIndex(int sampleRate) {
        for (int i = 0; i < SAMPLE_RATES.length; i++) {
            if (SAMPLE_RATES[i] == sampleRate) {
                return i;
            }
        }
        throw new IllegalArgumentException("VBAN has no code for " + sampleRate + " Hz");
    }
}
