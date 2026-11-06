package dev.trugamr.setu.server;

import dev.trugamr.setu.protocol.StreamProtocol;

import java.io.IOException;
import java.net.InetSocketAddress;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.function.LongSupplier;

/**
 * Who the session streams to, and for how long:
 *
 * - the Setu receiver that said HELLO most recently, until its hellos stop for
 *   {@link StreamProtocol#STALE_RECEIVER_MS} or a send to it fails;
 * - the VBAN target the agent set, until it is changed or cleared. VBAN has no handshake, so a
 *   failed send never drops it: nothing would bring it back.
 *
 * A route keeps its identity while it stays the same, so its datagram numbering carries on.
 * Thread-safe: the receive loop changes the routes and the streamer reads them.
 */
final class Routes {

    private final StreamProtocol.Format format;
    private final PacketFormat setu;
    private final LongSupplier clockMs;

    // Guarded by this.
    private Route receiver;
    private String receiverName;
    private long lastHelloMs;
    private StreamProtocol.VbanTarget vbanTarget;
    private Route vban;
    /** Whether a send to the current VBAN target failed, so the failure is logged once. */
    private boolean vbanFailed;
    private boolean stopped;

    Routes(StreamProtocol.Format format, LongSupplier clockMs) {
        this.format = format;
        this.setu = PacketFormat.setu(format);
        this.clockMs = clockMs;
    }

    synchronized void hello(InetSocketAddress from, String name) {
        if (liveReceiver() == null || !from.equals(receiver.address)) {
            Log.info("receivers: streaming to " + name + " (" + from + ")");
            receiver = new Route(from, setu, format.frameBytes(), name + " (" + from + ")");
        }
        receiverName = name;
        lastHelloMs = clockMs.getAsLong();
        notifyAll();
    }

    /**
     * Sends VBAN to {@code target}, or stops sending it when null. Throws
     * {@link IllegalArgumentException} for a target the stream's format cannot be sent to.
     */
    synchronized void setVban(StreamProtocol.VbanTarget target) {
        if (target == null ? vbanTarget == null : target.equals(vbanTarget)) {
            return;
        }
        vban = target == null ? null
                : new Route(target.address, PacketFormat.vban(format, target.streamName), format.frameBytes(),
                        "VBAN " + target);
        vbanTarget = target;
        vbanFailed = false;
        Log.info(target == null ? "receivers: VBAN off" : "receivers: sending VBAN " + target);
        notifyAll();
    }

    /** The routes to stream to now; empty when there are none or the session stopped. */
    synchronized List<Route> active() {
        if (stopped) {
            return Collections.emptyList();
        }
        List<Route> routes = new ArrayList<>(2);
        Route live = liveReceiver();
        if (live != null) {
            routes.add(live);
        }
        if (vban != null) {
            routes.add(vban);
        }
        return routes;
    }

    /** Waits for a route or a stop, and returns {@link #active()}: empty means stopped. */
    synchronized List<Route> awaitActive() throws InterruptedException {
        List<Route> routes;
        while ((routes = active()).isEmpty() && !stopped) {
            wait();
        }
        return routes;
    }

    /** A send to {@code route} failed. */
    synchronized void unreachable(Route route, IOException e) {
        if (route == receiver) {
            Log.info("receivers: cannot reach " + route + ", dropping it until it says hello: " + e.getMessage());
            receiver = null;
        } else if (route == vban && !vbanFailed) {
            Log.info("receivers: cannot reach " + route + ", still trying: " + e.getMessage());
            vbanFailed = true;
        }
    }

    synchronized StreamProtocol.Status status() {
        Route live = liveReceiver();
        List<StreamProtocol.Receiver> receivers = live == null
                ? Collections.emptyList()
                : Collections.singletonList(new StreamProtocol.Receiver(receiverName, address(live.address)));
        return new StreamProtocol.Status(format.blockFrames, receivers, vbanTarget);
    }

    synchronized boolean isStopped() {
        return stopped;
    }

    synchronized void stop() {
        stopped = true;
        notifyAll();
    }

    /** The Setu receiver, or null when there is none or its hellos stopped. */
    private Route liveReceiver() {
        if (receiver == null || clockMs.getAsLong() - lastHelloMs > StreamProtocol.STALE_RECEIVER_MS) {
            return null;
        }
        return receiver;
    }

    private static String address(InetSocketAddress a) {
        return a.getAddress().getHostAddress() + ":" + a.getPort();
    }
}
