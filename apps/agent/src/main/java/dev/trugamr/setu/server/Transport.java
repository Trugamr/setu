package dev.trugamr.setu.server;

import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.SocketException;
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

    /**
     * A UDP socket bound to a local port. Its datagrams carry DSCP CS6, so the headset's Wi-Fi
     * sends them in the voice access category (WMM AC_VO) ahead of bulk traffic. The Quest's
     * Qualcomm driver puts CS6 and EF alike in voice. CS6 is for the access point's leg to a PC
     * on Wi-Fi: the precedence mapping most access points use puts CS6 in voice and EF in video.
     * An access point following RFC 8325 puts CS6 in best effort instead.
     */
    final class Udp implements Transport {
        /** DSCP CS6 in the upper six bits of the IP traffic class. */
        private static final int TRAFFIC_CLASS = 0xC0;

        private final DatagramSocket socket;
        private final byte[] in = new byte[2048];
        private final DatagramPacket packet = new DatagramPacket(in, in.length);

        Udp(int port) throws IOException {
            socket = new DatagramSocket(port);
            try {
                socket.setTrafficClass(TRAFFIC_CLASS);
            } catch (SocketException e) {
                // Unmarked datagrams arrive all the same, queued with everything else.
                Log.info("udp: could not mark datagrams for the voice queue: " + e);
            }
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
