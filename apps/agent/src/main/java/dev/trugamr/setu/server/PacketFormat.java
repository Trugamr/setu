package dev.trugamr.setu.server;

import dev.trugamr.setu.protocol.StreamProtocol;
import dev.trugamr.setu.protocol.Vban;

/**
 * A wire format for audio datagrams: Setu's AUDIO for the desktop receiver, or VBAN for any VBAN
 * receiver. A {@link Route} sends every capture block in its format, split into as many
 * datagrams as the format needs.
 */
interface PacketFormat {

    /** Bytes per frame of the PCM it carries. */
    int frameBytes();

    /** Frames one datagram carries at most. */
    int maxFrames();

    /** A buffer that holds any datagram of this format. */
    byte[] newPacket();

    /**
     * Writes datagram {@code index} of a route (counted from 0) into
     * {@code packet}: {@code length} bytes of {@code pcm} from {@code offset}, whose first frame
     * is {@code framePosition}. Returns the datagram length.
     */
    int write(byte[] packet, int index, long framePosition, byte[] pcm, int offset, int length);

    static PacketFormat setu(StreamProtocol.Format format) {
        int maxFrames = StreamProtocol.maxAudioPcmBytes(format) / format.frameBytes();
        return new PacketFormat() {
            @Override
            public int frameBytes() {
                return format.frameBytes();
            }

            @Override
            public int maxFrames() {
                return maxFrames;
            }

            @Override
            public byte[] newPacket() {
                return StreamProtocol.newAudioPacket(format);
            }

            @Override
            public int write(byte[] packet, int index, long framePosition, byte[] pcm, int offset, int length) {
                return StreamProtocol.audio(packet, index, framePosition, pcm, offset, length);
            }
        };
    }

    /** Throws {@link IllegalArgumentException} for a format or name VBAN cannot carry. */
    static PacketFormat vban(StreamProtocol.Format format, String streamName) {
        Vban.Encoder encoder = new Vban.Encoder(format, streamName);
        return new PacketFormat() {
            @Override
            public int frameBytes() {
                return format.frameBytes();
            }

            @Override
            public int maxFrames() {
                return encoder.maxFrames();
            }

            @Override
            public byte[] newPacket() {
                return encoder.newPacket();
            }

            @Override
            public int write(byte[] packet, int index, long framePosition, byte[] pcm, int offset, int length) {
                return encoder.audio(packet, index, pcm, offset, length);
            }
        };
    }
}
