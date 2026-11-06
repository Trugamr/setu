package dev.trugamr.setu.server;

import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.SocketTimeoutException;
import java.util.Arrays;

/**
 * Datagrams in and out for the {@link ReceiverSession}. {@link Udp} is the adapter on the
 * headset; tests use an in-memory one. {@link #send} may be called from several threads.
 */
interface Transport {

    /** Waits up to {@code timeoutMs} for a datagram; returns null when none arrived. */
    Datagram receive(int timeoutMs) throws IOException;

    void send(byte[] data, int length, InetSocketAddress to) throws IOException;

    /** A received datagram, exactly as long as its data. */
    final class Datagram {
        final byte[] data;
        final InetSocketAddress from;

        Datagram(byte[] data, InetSocketAddress from) {
            this.data = data;
            this.from = from;
        }
    }

    /** A UDP socket bound to a local port. */
    final class Udp implements Transport {
        private final DatagramSocket socket;
        private final byte[] in = new byte[2048];
        private final DatagramPacket packet = new DatagramPacket(in, in.length);

        Udp(int port) throws IOException {
            socket = new DatagramSocket(port);
        }

        @Override
        public Datagram receive(int timeoutMs) throws IOException {
            socket.setSoTimeout(timeoutMs);
            packet.setLength(in.length);
            try {
                socket.receive(packet);
            } catch (SocketTimeoutException e) {
                return null;
            }
            return new Datagram(Arrays.copyOf(in, packet.getLength()), (InetSocketAddress) packet.getSocketAddress());
        }

        @Override
        public void send(byte[] data, int length, InetSocketAddress to) throws IOException {
            socket.send(new DatagramPacket(data, length, to));
        }
    }
}
