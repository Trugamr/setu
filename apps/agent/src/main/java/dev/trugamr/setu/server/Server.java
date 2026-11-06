package dev.trugamr.setu.server;

import android.content.Context;
import android.net.LocalServerSocket;
import android.net.LocalSocket;
import android.net.wifi.WifiManager;

import dev.trugamr.setu.protocol.StreamProtocol;

import java.io.IOException;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.List;

/**
 * Runs on the headset as the shell user:
 *
 *   CLASSPATH=$(pm path dev.trugamr.setu | cut -d: -f2) app_process / dev.trugamr.setu.server.Server <command> ...
 *
 *   udp [port] [blockFrames]        serve receivers over UDP until STOP (Wi-Fi)
 *   adb [blockFrames]               stream to one client over the "setu" abstract socket (adb forward)
 *   probe [clicks] [blockFrames]    measure headset-side capture latency
 *   bursts [seconds] [blockFrames]  report how capture blocks arrive
 *   adb-wifi-trust <bssid>          always allow wireless debugging on this network, and enable it
 *   adb-revoke-keys                 revoke every adb authorization (USB and wireless)
 *
 * blockFrames sets the capture period (96 frames = 2 ms, see PlaybackCapture). The wire format
 * is in protocol/README.md.
 */
public final class Server {

    private static final int DEFAULT_BLOCK_FRAMES = 96;
    private static final String SOCKET_NAME = "setu";
    /** Held for the life of the process so the locks are not released by the garbage collector. */
    private static final List<WifiManager.WifiLock> WIFI_LOCKS = new ArrayList<>();

    private Server() {
    }

    public static void main(String[] args) throws Exception {
        ShellRuntime.init();

        String command = args.length > 0 ? args[0] : "udp";
        switch (command) {
            case "udp":
                serveUdp(intArg(args, 1, StreamProtocol.UDP_PORT), intArg(args, 2, DEFAULT_BLOCK_FRAMES));
                return;
            case "adb":
                serveAdb(intArg(args, 1, DEFAULT_BLOCK_FRAMES));
                return;
            case "probe":
                LatencyProbe.run(intArg(args, 1, 8), intArg(args, 2, DEFAULT_BLOCK_FRAMES));
                return;
            case "bursts":
                BurstProbe.run(intArg(args, 1, 5), intArg(args, 2, DEFAULT_BLOCK_FRAMES));
                return;
            case "adb-wifi-trust":
                AdbWifi.trust(args[1]);
                return;
            case "adb-revoke-keys":
                AdbWifi.revokeKeys();
                return;
            default:
                throw new IllegalArgumentException("unknown command: " + command);
        }
    }

    private static int intArg(String[] args, int index, int fallback) {
        return args.length > index ? Integer.parseInt(args[index]) : fallback;
    }

    private static void serveUdp(int port, int blockFrames) throws IOException, InterruptedException {
        holdWifiLocks();
        Transport transport = new Transport.Udp(port);
        Log.info("udp: listening on port " + port + " (block " + blockFrames + " frames)");
        new ReceiverSession(transport, PlaybackCapture.source(blockFrames), () -> System.nanoTime() / 1_000_000).run();
        Log.info("udp: stopped");
    }

    private static void serveAdb(int blockFrames) throws IOException {
        LocalServerSocket serverSocket = new LocalServerSocket(SOCKET_NAME);
        Log.info("adb: listening on @" + SOCKET_NAME + " (block " + blockFrames + " frames)");
        // One client per server process; the PC side restarts the server to reconnect.
        try (LocalSocket socket = serverSocket.accept(); Capture capture = PlaybackCapture.open(blockFrames)) {
            serverSocket.close();
            OutputStream out = socket.getOutputStream();
            out.write(StreamProtocol.format(PlaybackCapture.source(blockFrames).format()));
            while (true) {
                Capture.Block block = capture.read();
                out.write(block.data, 0, block.length);
            }
        } catch (IOException e) {
            Log.info("adb: stopped: " + e.getMessage());
        }
    }

    /**
     * Asks Wi-Fi to stay out of power save so datagrams leave as they are sent instead of in
     * clumps. Android only honors low-latency mode for foreground apps, so both modes are tried
     * and the result is logged.
     */
    private static void holdWifiLocks() {
        try {
            WifiManager wifi = (WifiManager) ShellRuntime.context().getSystemService(Context.WIFI_SERVICE);
            int[] modes = {WifiManager.WIFI_MODE_FULL_LOW_LATENCY, 3 /* WIFI_MODE_FULL_HIGH_PERF */};
            for (int mode : modes) {
                WifiManager.WifiLock lock = wifi.createWifiLock(mode, "setu:" + mode);
                lock.setReferenceCounted(false);
                lock.acquire();
                Log.info("udp: wifi lock mode " + mode + " held=" + lock.isHeld());
                WIFI_LOCKS.add(lock);
            }
        } catch (RuntimeException e) {
            Log.info("udp: could not hold wifi locks: " + e);
        }
    }
}
