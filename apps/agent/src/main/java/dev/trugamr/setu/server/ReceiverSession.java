package dev.trugamr.setu.server;

import dev.trugamr.setu.protocol.StreamProtocol;

import java.io.IOException;
import java.net.InetSocketAddress;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.LongSupplier;

/**
 * Serves receivers over a {@link Transport} (protocol/README.md, "UDP" and "VBAN"):
 *
 * - answers each HELLO with FORMAT and streams AUDIO to the Setu receiver that said hello most
 *   recently;
 * - sends VBAN to the target the agent set, alongside;
 * - holds a capture open only while it has someone to stream to ({@link Routes});
 * - answers STATUS_REQUEST and obeys STOP and SET_VBAN, from loopback senders only.
 *
 * Network errors never end the session. {@link #run()} returns after STOP, or throws when the
 * capture fails.
 */
final class ReceiverSession {

    /** How often the receive loop re-checks for a stop. */
    private static final int POLL_MS = 250;
    /** How long a stop waits for the streamer to release its capture. */
    private static final int STOP_TIMEOUT_MS = 1000;

    private final Transport transport;
    private final Capture.Source captures;
    private final Routes routes;
    private final byte[] formatMessage;

    // Guarded by this.
    private IOException failure;

    ReceiverSession(Transport transport, Capture.Source captures, LongSupplier clockMs) {
        this.transport = transport;
        this.captures = captures;
        this.routes = new Routes(captures.format(), clockMs);
        this.formatMessage = StreamProtocol.format(captures.format());
    }

    /**
     * Sends VBAN to {@code target}, or stops when null; the agent does the same with SET_VBAN.
     * Throws {@link IllegalArgumentException} for a target the stream cannot be sent to.
     */
    void setVban(StreamProtocol.VbanTarget target) {
        routes.setVban(target);
    }

    void run() throws IOException, InterruptedException {
        Thread streamer = new Thread(this::stream, "setu-stream");
        // A capture read that never returns must not keep the process alive after a stop.
        streamer.setDaemon(true);
        streamer.start();
        try {
            while (!routes.isStopped()) {
                Transport.Datagram datagram = transport.receive(POLL_MS);
                if (datagram != null) {
                    handle(datagram);
                }
            }
        } finally {
            routes.stop();
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
        int type = StreamProtocol.type(d.data, d.data.length);
        if (type == StreamProtocol.HELLO) {
            String name;
            try {
                name = StreamProtocol.parseHello(d.data, d.data.length);
            } catch (IllegalArgumentException e) {
                return;
            }
            // FORMAT goes out before the streamer is woken, so it precedes the first AUDIO.
            if (reply(formatMessage, d.from)) {
                routes.hello(d.from, name);
            }
            return;
        }
        // The rest is the agent's, on this headset.
        if (!d.from.getAddress().isLoopbackAddress()) {
            return;
        }
        switch (type) {
            case StreamProtocol.STATUS_REQUEST:
                reply(StreamProtocol.status(routes.status()), d.from);
                break;
            case StreamProtocol.STOP:
                Log.info("receivers: stop requested");
                routes.stop();
                break;
            case StreamProtocol.SET_VBAN:
                try {
                    routes.setVban(StreamProtocol.parseSetVban(d.data, d.data.length));
                } catch (IllegalArgumentException e) {
                    Log.info("receivers: ignoring SET_VBAN: " + e.getMessage());
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

    private void stream() {
        try {
            List<Route> targets;
            while (!(targets = routes.awaitActive()).isEmpty()) {
                try (Capture capture = captures.open()) {
                    // Datagrams sent to each route during this capture, which numbers the next one.
                    Map<Route, Integer> sent = new HashMap<>();
                    while (!targets.isEmpty()) {
                        Capture.Block block = capture.read();
                        targets = routes.active();
                        for (Route route : targets) {
                            try {
                                sent.put(route, route.send(transport, block, sent.getOrDefault(route, 0)));
                            } catch (IOException e) {
                                routes.unreachable(route, e);
                            }
                        }
                    }
                }
                if (!routes.isStopped()) {
                    Log.info("receivers: nobody to stream to, released capture");
                }
            }
        } catch (IOException e) {
            synchronized (this) {
                failure = e;
            }
            routes.stop();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            routes.stop();
        }
    }
}
