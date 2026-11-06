package dev.trugamr.setu.server;

import java.io.IOException;
import java.net.InetSocketAddress;

/**
 * One place the session streams to: an address and the {@link PacketFormat} it reads. Only the
 * streamer thread sends, so a route reuses one packet buffer.
 */
final class Route {

    final InetSocketAddress address;
    private final PacketFormat format;
    private final int frameBytes;
    private final int maxBytes;
    private final byte[] packet;
    private final String description;

    Route(InetSocketAddress address, PacketFormat format, int frameBytes, String description) {
        this.address = address;
        this.format = format;
        this.frameBytes = frameBytes;
        this.maxBytes = format.maxFrames() * frameBytes;
        this.packet = format.newPacket();
        this.description = description;
    }

    /**
     * Sends {@code block} as datagrams numbered from {@code index}, splitting it to fit the
     * format. Returns the index of the next datagram.
     */
    int send(Transport transport, Capture.Block block, int index) throws IOException {
        for (int off = 0; off < block.length; off += maxBytes) {
            int chunk = Math.min(block.length - off, maxBytes);
            int length = format.write(packet, index++, block.framePosition + off / frameBytes, block.data, off, chunk);
            transport.send(packet, length, address);
        }
        return index;
    }

    @Override
    public String toString() {
        return description;
    }
}
