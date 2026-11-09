package dev.trugamr.setu.server;

import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Set;

/**
 * Ends another Setu server that holds the UDP port, so a new one can bind it. Starting a server
 * replaces the running one: a server that stopped but never exited, or one started from adb,
 * would otherwise keep the port and make every Start fail.
 */
final class PortTakeover {

    private static final String[] UDP_TABLES = {"/proc/net/udp", "/proc/net/udp6"};
    /** Columns of /proc/net/udp: sl, local_address, rem_address, st, queues, tr, retrnsmt, uid, timeout, inode. */
    private static final int LOCAL_ADDRESS = 1;
    private static final int INODE = 9;

    private PortTakeover() {
    }

    /** The inodes of the sockets bound to {@code port} in a /proc/net/udp or udp6 table. */
    static Set<Long> socketInodes(String table, int port) {
        String suffix = String.format(Locale.ROOT, ":%04X", port);
        Set<Long> inodes = new HashSet<>();
        for (String line : table.split("\n")) {
            String[] columns = line.trim().split("\\s+");
            if (columns.length <= INODE || !columns[LOCAL_ADDRESS].toUpperCase(Locale.ROOT).endsWith(suffix)) {
                continue;
            }
            try {
                inodes.add(Long.parseLong(columns[INODE]));
            } catch (NumberFormatException e) {
                // The header line.
            }
        }
        return inodes;
    }

    /** Whether a {@code /proc/<pid>/cmdline}, its arguments separated by NULs, runs a Setu server. */
    static boolean isServer(String cmdline) {
        return cmdline.contains("\0" + Server.class.getName() + "\0");
    }

    /** Ends every other Setu server holding {@code port}, and returns their pids. */
    static List<Integer> endHolders(int port) throws IOException {
        Set<String> sockets = new HashSet<>();
        for (String table : UDP_TABLES) {
            for (long inode : socketInodes(read(new File(table)), port)) {
                sockets.add("socket:[" + inode + "]");
            }
        }
        List<Integer> ended = new ArrayList<>();
        if (sockets.isEmpty()) {
            return ended;
        }
        String self = new File("/proc/self").getCanonicalFile().getName();
        File[] processes = new File("/proc").listFiles((dir, name) -> name.matches("\\d+") && !name.equals(self));
        for (File process : processes == null ? new File[0] : processes) {
            if (holdsAny(process, sockets) && isServer(read(new File(process, "cmdline")))) {
                int pid = Integer.parseInt(process.getName());
                if (kill(pid)) {
                    ended.add(pid);
                }
            }
        }
        return ended;
    }

    private static boolean holdsAny(File process, Set<String> sockets) {
        File[] fds = new File(process, "fd").listFiles();
        if (fds == null) {
            // Another user's process, or gone.
            return false;
        }
        for (File fd : fds) {
            try {
                if (sockets.contains(Files.readSymbolicLink(fd.toPath()).toString())) {
                    return true;
                }
            } catch (IOException e) {
                // Closed while listing.
            }
        }
        return false;
    }

    /** Whether {@code kill} ended {@code pid}: it fails for a process that exited first. */
    private static boolean kill(int pid) throws IOException {
        try {
            return new ProcessBuilder("kill", "-9", String.valueOf(pid)).start().waitFor() == 0;
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return false;
        }
    }

    private static String read(File file) {
        try {
            return new String(Files.readAllBytes(file.toPath()), StandardCharsets.UTF_8);
        } catch (IOException e) {
            return "";
        }
    }
}
