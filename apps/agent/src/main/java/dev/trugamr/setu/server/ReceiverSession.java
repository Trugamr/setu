package dev.trugamr.setu.server;

import dev.trugamr.setu.protocol.StreamProtocol;

import java.io.IOException;
import java.net.InetSocketAddress;
import java.util.Collections;
import java.util.List;
import java.util.function.LongSupplier;

/**
 * Serves receivers over a {@link Transport} (protocol/README.md, "UDP"):
 *
 * - answers each HELLO with FORMAT and streams AUDIO to the receiver that said hello most
 *   recently;
 * - holds a capture open only while that receiver keeps saying hello, and releases it
 *   {@link StreamProtocol#STALE_RECEIVER_MS} after the hellos stop;
 * - answers STATUS_REQUEST and obeys STOP, from loopback senders only.
 *
 * A receiver that cannot be sent to is dropped until its next HELLO; network errors never end
 * the session. {@link #run()} returns after STOP, or throws when the capture fails.
 */
final class ReceiverSession {

    /** How often the receive loop re-checks for a stop. */
    private static final int POLL_MS = 250;
    /** How long a stop waits for the streamer to release its capture. */
    private static final int STOP_TIMEOUT_MS = 1000;

    private final Transport transport;
    private final Capture.Source captures;
    private final LongSupplier clockMs;
    private final StreamProtocol.Format format;
    private final byte[] formatMessage;
    private final byte[] packet;

    // Guarded by this.
    private InetSocketAddress receiver;
    private String receiverName;
    private long lastHelloMs;
    private boolean stopped;
    private IOException failure;

    ReceiverSession(Transport transport, Capture.Source captures, LongSupplier clockMs) {
        this.transport = transport;
        this.captures = captures;
        this.clockMs = clockMs;
        this.format = captures.format();
        this.formatMessage = StreamProtocol.format(format);
        this.packet = StreamProtocol.newAudioPacket(format);
    }

    void run() throws IOException, InterruptedException {
        Thread streamer = new Thread(this::stream, "setu-stream");
        // A capture read that never returns must not keep the process alive after a stop.
        streamer.setDaemon(true);
        streamer.start();
        try {
            while (!isStopped()) {
                Transport.Datagram datagram = transport.receive(POLL_MS);
                if (datagram != null) {
                    handle(datagram);
                }
            }
        } finally {
            stop();
            streamer.join(STOP_TIMEOUT_MS);
            if (streamer.isAlive()) {
                Log.info("receivers: capture read did not return; stopping without releasing it");
            }
        }
        synchronized (this) {
            if (failure != null) {
                throw failure;
            }
        }
    }

    private void handle(Transport.Datagram d) {
        boolean local = d.from.getAddress().isLoopbackAddress();
        switch (StreamProtocol.type(d.data, d.data.length)) {
            case StreamProtocol.HELLO:
                String name;
                try {
                    name = StreamProtocol.parseHello(d.data, d.data.length);
                } catch (IllegalArgumentException e) {
                    return;
                }
                // FORMAT goes out before the streamer is woken, so it precedes the first AUDIO.
                if (reply(formatMessage, d.from)) {
                    hello(d.from, name);
                }
                break;
            case StreamProtocol.STATUS_REQUEST:
                if (local) {
                    reply(StreamProtocol.status(status()), d.from);
                }
                break;
            case StreamProtocol.STOP:
                if (local) {
                    Log.info("receivers: stop requested");
                    stop();
                }
                break;
            default:
                break;
        }
    }

    /** Sends a reply; returns false, after logging, when the sender cannot be reached. */
    private boolean reply(byte[] message, InetSocketAddress to) {
        try {
            transport.send(message, message.length, to);
            return true;
        } catch (IOException e) {
            Log.info("receivers: cannot reply to " + to + ": " + e.getMessage());
            return false;
        }
    }

    private synchronized void hello(InetSocketAddress from, String name) {
        if (current() == null || !from.equals(receiver)) {
            Log.info("receivers: streaming to " + name + " (" + from + ")");
        }
        receiver = from;
        receiverName = name;
        lastHelloMs = clockMs.getAsLong();
        notifyAll();
    }

    private synchronized StreamProtocol.Status status() {
        InetSocketAddress current = current();
        List<StreamProtocol.Receiver> receivers = current == null
                ? Collections.emptyList()
                : Collections.singletonList(new StreamProtocol.Receiver(receiverName, address(current)));
        return new StreamProtocol.Status(format.blockFrames, receivers);
    }

    private static String address(InetSocketAddress a) {
        return a.getAddress().getHostAddress() + ":" + a.getPort();
    }

    /** The receiver to stream to, or null when stopped or nobody has said hello recently. */
    private synchronized InetSocketAddress current() {
        if (stopped || receiver == null || clockMs.getAsLong() - lastHelloMs > StreamProtocol.STALE_RECEIVER_MS) {
            return null;
        }
        return receiver;
    }

    /** Stops streaming to {@code target} until it says hello again. */
    private synchronized void forget(InetSocketAddress target) {
        if (target.equals(receiver)) {
            receiver = null;
        }
    }

    private synchronized boolean isStopped() {
        return stopped;
    }

    private synchronized void stop() {
        stopped = true;
        notifyAll();
    }

    /** Waits for a hello or a stop; both notify. Returns null when stopped. */
    private synchronized InetSocketAddress awaitReceiver() throws InterruptedException {
        InetSocketAddress target;
        while ((target = current()) == null && !stopped) {
            wait();
        }
        return target;
    }

    private void stream() {
        try {
            InetSocketAddress target;
            while ((target = awaitReceiver()) != null) {
                try (Capture capture = captures.open()) {
                    int sequence = 0;
                    while (target != null) {
                        Capture.Block block = capture.read();
                        target = current();
                        if (target == null) {
                            continue;
                        }
                        try {
                            sequence = send(block, target, sequence);
                        } catch (IOException e) {
                            Log.info("receivers: cannot reach " + target + ", dropping it: " + e.getMessage());
                            forget(target);
                            target = null;
                        }
                    }
                }
                if (!isStopped()) {
                    Log.info("receivers: no receiver, released capture");
                }
            }
        } catch (IOException e) {
            synchronized (this) {
                failure = e;
            }
            stop();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            stop();
        }
    }

    /** Splits a block into AUDIO datagrams; returns the next sequence number. */
    private int send(Capture.Block block, InetSocketAddress target, int sequence) throws IOException {
        int frameBytes = format.frameBytes();
        int maxBytes = StreamProtocol.maxAudioPcmBytes(format);
        for (int off = 0; off < block.length; off += maxBytes) {
            int chunk = Math.min(block.length - off, maxBytes);
            int length = StreamProtocol.audio(packet, sequence++, block.framePosition + off / frameBytes,
                    block.data, off, chunk);
            transport.send(packet, length, target);
        }
        return sequence;
    }
}
