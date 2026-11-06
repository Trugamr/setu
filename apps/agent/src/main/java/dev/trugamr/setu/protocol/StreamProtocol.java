package dev.trugamr.setu.protocol;

import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.UnknownHostException;
import java.nio.BufferUnderflowException;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Objects;

/**
 * Encodes and decodes Setu's stream protocol messages (spec: protocol/README.md). Pure byte
 * work with no I/O; the capture server and the agent share it.
 *
 * Decoders take a buffer and the message length, and throw {@link IllegalArgumentException}
 * when the message is truncated or is not the type asked for.
 */
public final class StreamProtocol {

    public static final int VERSION = 1;
    public static final int UDP_PORT = 27184;
    /** A receiver that has not sent a HELLO for this long stops being streamed to. */
    public static final int STALE_RECEIVER_MS = 3000;
    /** Frames per AUDIO datagram, so that a datagram fits a 1500-byte MTU. */
    private static final int MAX_AUDIO_FRAMES = 340;
    private static final int AUDIO_HEADER_BYTES = 16;

    public static final int HELLO = 1;
    public static final int FORMAT = 2;
    public static final int AUDIO = 3;
    public static final int STATUS_REQUEST = 4;
    public static final int STATUS = 5;
    public static final int STOP = 6;
    public static final int SET_VBAN = 7;

    private static final int HEADER_BYTES = 4;
    private static final byte MAGIC_0 = 'S';
    private static final byte MAGIC_1 = 'T';

    private StreamProtocol() {
    }

    /** The message type, or -1 when the data is not a version 1 Setu message. */
    public static int type(byte[] data, int length) {
        if (length < HEADER_BYTES || data[0] != MAGIC_0 || data[1] != MAGIC_1 || data[2] != VERSION) {
            return -1;
        }
        return data[3] & 0xff;
    }

    /** A message with no payload: STATUS_REQUEST or STOP. */
    public static byte[] empty(int type) {
        return new Writer(new byte[HEADER_BYTES], type).bytes();
    }

    public static byte[] hello(String name) {
        byte[] utf8 = name.getBytes(StandardCharsets.UTF_8);
        return new Writer(new byte[HEADER_BYTES + 2 + utf8.length], HELLO).string(utf8).bytes();
    }

    public static String parseHello(byte[] data, int length) {
        return new Reader(data, length, HELLO).string();
    }

    /** Sample format of the stream, sent in FORMAT. */
    public static final class Format {
        public final int sampleRate;
        public final int channels;
        public final int bytesPerSample;
        public final int blockFrames;

        public Format(int sampleRate, int channels, int bytesPerSample, int blockFrames) {
            this.sampleRate = sampleRate;
            this.channels = channels;
            this.bytesPerSample = bytesPerSample;
            this.blockFrames = blockFrames;
        }

        public int frameBytes() {
            return channels * bytesPerSample;
        }

        @Override
        public boolean equals(Object o) {
            if (!(o instanceof Format)) {
                return false;
            }
            Format f = (Format) o;
            return sampleRate == f.sampleRate && channels == f.channels && bytesPerSample == f.bytesPerSample
                    && blockFrames == f.blockFrames;
        }

        @Override
        public int hashCode() {
            return Objects.hash(sampleRate, channels, bytesPerSample, blockFrames);
        }

        @Override
        public String toString() {
            return sampleRate + " Hz, " + channels + " ch, " + bytesPerSample + " B, block " + blockFrames;
        }
    }

    public static byte[] format(Format format) {
        return new Writer(new byte[HEADER_BYTES + 12], FORMAT)
                .u32(format.sampleRate).u16(format.channels).u16(format.bytesPerSample).u32(format.blockFrames)
                .bytes();
    }

    public static Format parseFormat(byte[] data, int length) {
        Reader r = new Reader(data, length, FORMAT);
        return new Format(r.u32(), r.u16(), r.u16(), r.u32());
    }

    /** PCM bytes one AUDIO datagram carries at most; a capture block larger than this is split. */
    public static int maxAudioPcmBytes(Format format) {
        return MAX_AUDIO_FRAMES * format.frameBytes();
    }

    /** A buffer that holds any AUDIO message of this format, for {@link #audio}. */
    public static byte[] newAudioPacket(Format format) {
        return new byte[AUDIO_HEADER_BYTES + maxAudioPcmBytes(format)];
    }

    /**
     * Writes an AUDIO message into {@code packet} (from {@link #newAudioPacket}): the header,
     * then {@code pcmLength} bytes of {@code pcm} from {@code pcmOffset}, at most
     * {@link #maxAudioPcmBytes}. Returns the message length.
     */
    public static int audio(byte[] packet, int sequence, long framePosition, byte[] pcm, int pcmOffset, int pcmLength) {
        new Writer(packet, AUDIO).u32(sequence).u64(framePosition);
        System.arraycopy(pcm, pcmOffset, packet, AUDIO_HEADER_BYTES, pcmLength);
        return AUDIO_HEADER_BYTES + pcmLength;
    }

    public static int audioSequence(byte[] data, int length) {
        Reader r = new Reader(data, length, AUDIO);
        return r.u32();
    }

    public static long audioFramePosition(byte[] data, int length) {
        Reader r = new Reader(data, length, AUDIO);
        r.u32();
        return r.u64();
    }

    /** A receiver as STATUS reports it. */
    public static final class Receiver {
        public final String name;
        /** "host:port" as the server sees it. */
        public final String address;

        public Receiver(String name, String address) {
            this.name = name;
            this.address = address;
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof Receiver && name.equals(((Receiver) o).name) && address.equals(((Receiver) o).address);
        }

        @Override
        public int hashCode() {
            return Objects.hash(name, address);
        }

        @Override
        public String toString() {
            return name + " (" + address + ")";
        }
    }

    /** Where the server sends VBAN: a receiver's address and the stream name it listens for. */
    public static final class VbanTarget {
        public final InetSocketAddress address;
        public final String streamName;

        /** Throws {@link IllegalArgumentException} for a name VBAN cannot carry ({@link Vban#checkStreamName}). */
        public VbanTarget(InetSocketAddress address, String streamName) {
            if (address.isUnresolved()) {
                throw new IllegalArgumentException("unresolved VBAN address: " + address);
            }
            this.address = address;
            this.streamName = Vban.checkStreamName(streamName);
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof VbanTarget && address.equals(((VbanTarget) o).address)
                    && streamName.equals(((VbanTarget) o).streamName);
        }

        @Override
        public int hashCode() {
            return Objects.hash(address, streamName);
        }

        @Override
        public String toString() {
            return "\"" + streamName + "\" to " + address.getAddress().getHostAddress() + ":" + address.getPort();
        }
    }

    /** What the server is doing, sent in STATUS. */
    public static final class Status {
        public final int blockFrames;
        public final List<Receiver> receivers;
        /** Null when VBAN is off. */
        public final VbanTarget vban;

        public Status(int blockFrames, List<Receiver> receivers, VbanTarget vban) {
            this.blockFrames = blockFrames;
            this.receivers = Collections.unmodifiableList(new ArrayList<>(receivers));
            this.vban = vban;
        }

        @Override
        public boolean equals(Object o) {
            if (!(o instanceof Status)) {
                return false;
            }
            Status s = (Status) o;
            return blockFrames == s.blockFrames && receivers.equals(s.receivers) && Objects.equals(vban, s.vban);
        }

        @Override
        public int hashCode() {
            return Objects.hash(blockFrames, receivers, vban);
        }

        @Override
        public String toString() {
            return "block " + blockFrames + ", receivers " + receivers + ", VBAN " + (vban == null ? "off" : vban);
        }
    }

    public static byte[] status(Status status) {
        List<byte[]> strings = new ArrayList<>();
        int size = 6 + vbanTargetBytes(status.vban);
        for (Receiver receiver : status.receivers) {
            byte[] name = receiver.name.getBytes(StandardCharsets.UTF_8);
            byte[] address = receiver.address.getBytes(StandardCharsets.UTF_8);
            strings.add(name);
            strings.add(address);
            size += 4 + name.length + address.length;
        }
        Writer w = new Writer(new byte[HEADER_BYTES + size], STATUS).u32(status.blockFrames).u16(status.receivers.size());
        for (byte[] s : strings) {
            w.string(s);
        }
        return w.vbanTarget(status.vban).bytes();
    }

    public static Status parseStatus(byte[] data, int length) {
        Reader r = new Reader(data, length, STATUS);
        int blockFrames = r.u32();
        int count = r.u16();
        List<Receiver> receivers = new ArrayList<>(count);
        for (int i = 0; i < count; i++) {
            receivers.add(new Receiver(r.string(), r.string()));
        }
        return new Status(blockFrames, receivers, r.vbanTarget());
    }

    /** SET_VBAN: send VBAN to {@code target}, or stop sending it when null. */
    public static byte[] setVban(VbanTarget target) {
        return new Writer(new byte[HEADER_BYTES + vbanTargetBytes(target)], SET_VBAN).vbanTarget(target).bytes();
    }

    /** The target a SET_VBAN carries; null turns VBAN off. */
    public static VbanTarget parseSetVban(byte[] data, int length) {
        return new Reader(data, length, SET_VBAN).vbanTarget();
    }

    private static int vbanTargetBytes(VbanTarget target) {
        if (target == null) {
            return 1;
        }
        return 1 + target.address.getAddress().getAddress().length + 2 + 2
                + target.streamName.getBytes(StandardCharsets.US_ASCII).length;
    }

    /** Writes a message into a buffer sized for it, header first. */
    private static final class Writer {
        private final ByteBuffer b;

        Writer(byte[] buffer, int type) {
            b = ByteBuffer.wrap(buffer).order(ByteOrder.LITTLE_ENDIAN);
            b.put(MAGIC_0).put(MAGIC_1).put((byte) VERSION).put((byte) type);
        }

        Writer u16(int v) {
            if (v < 0 || v > 0xffff) {
                throw new IllegalArgumentException("does not fit a u16: " + v);
            }
            b.putShort((short) v);
            return this;
        }

        Writer u32(int v) {
            b.putInt(v);
            return this;
        }

        Writer u64(long v) {
            b.putLong(v);
            return this;
        }

        Writer string(byte[] utf8) {
            u16(utf8.length);
            b.put(utf8);
            return this;
        }

        Writer vbanTarget(VbanTarget target) {
            if (target == null) {
                b.put((byte) 0);
                return this;
            }
            byte[] address = target.address.getAddress().getAddress();
            b.put((byte) address.length).put(address);
            u16(target.address.getPort());
            return string(target.streamName.getBytes(StandardCharsets.US_ASCII));
        }

        byte[] bytes() {
            return b.array();
        }
    }

    /** Reads a message's payload after checking its header; truncation is an IllegalArgumentException. */
    private static final class Reader {
        private final ByteBuffer b;

        Reader(byte[] data, int length, int expectedType) {
            int actual = type(data, length);
            if (actual != expectedType) {
                throw new IllegalArgumentException("expected message type " + expectedType + ", got " + actual);
            }
            b = ByteBuffer.wrap(data, HEADER_BYTES, length - HEADER_BYTES).order(ByteOrder.LITTLE_ENDIAN);
        }

        int u16() {
            try {
                return b.getShort() & 0xffff;
            } catch (BufferUnderflowException e) {
                throw truncated();
            }
        }

        int u32() {
            try {
                return b.getInt();
            } catch (BufferUnderflowException e) {
                throw truncated();
            }
        }

        long u64() {
            try {
                return b.getLong();
            } catch (BufferUnderflowException e) {
                throw truncated();
            }
        }

        String string() {
            int n = u16();
            if (n > b.remaining()) {
                throw truncated();
            }
            String s = new String(b.array(), b.position(), n, StandardCharsets.UTF_8);
            b.position(b.position() + n);
            return s;
        }

        VbanTarget vbanTarget() {
            int addressLength = u8();
            if (addressLength == 0) {
                return null;
            }
            if (addressLength != 4 && addressLength != 16) {
                throw new IllegalArgumentException("VBAN address of " + addressLength + " bytes");
            }
            byte[] address = new byte[addressLength];
            try {
                b.get(address);
            } catch (BufferUnderflowException e) {
                throw truncated();
            }
            int port = u16();
            try {
                // From raw bytes, so no name lookup happens.
                return new VbanTarget(new InetSocketAddress(InetAddress.getByAddress(address), port), string());
            } catch (UnknownHostException e) {
                throw new IllegalArgumentException(e);
            }
        }

        int u8() {
            try {
                return b.get() & 0xff;
            } catch (BufferUnderflowException e) {
                throw truncated();
            }
        }

        private IllegalArgumentException truncated() {
            return new IllegalArgumentException("message truncated at byte " + b.position());
        }
    }
}
