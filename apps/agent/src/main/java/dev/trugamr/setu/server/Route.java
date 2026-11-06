package dev.trugamr.setu.server;

import java.io.IOException;
import java.net.InetSocketAddress;

/**
 * One place the session streams to: an address and the {@link PacketFormat} it reads. Only the
 * streamer thread sends, so a route reuses one packet buffer and numbers its own datagrams.
 */
final class Route {

    final InetSocketAddress address;
    private final PacketFormat format;
    private final int maxBytes;
    private final byte[] packet;
    private final String description;
    /** The next datagram's index, from 0 when the route is made. Streamer thread only. */
    private int next;
    /** Whether a send failed, so {@link Routes} reports it once. Guarded by the Routes lock. */
    boolean failed;

    Route(InetSocketAddress address, PacketFormat format, String description) {
        this.address = address;
        this.format = format;
        this.maxBytes = format.maxFrames() * format.frameBytes();
        this.packet = format.newPacket();
        this.description = description;
    }

    /** Sends {@code block} as the route's next datagrams, split to fit the format. */
    void send(Transport transport, Capture.Block block) throws IOException {
        for (int off = 0; off < block.length; off += maxBytes) {
            int chunk = Math.min(block.length - off, maxBytes);
            long framePosition = block.framePosition + off / format.frameBytes();
            int length = format.write(packet, next++, framePosition, block.data, off, chunk);
            transport.send(packet, length, address);
        }
    }

    @Override
    public String toString() {
        return description;
    }
}
