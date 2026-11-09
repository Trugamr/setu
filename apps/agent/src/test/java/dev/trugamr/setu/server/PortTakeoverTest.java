package dev.trugamr.setu.server;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import java.util.Arrays;
import java.util.Collections;
import java.util.HashSet;

import org.junit.Test;

public class PortTakeoverTest {

    /** /proc/net/udp6 on a Quest 3 with a server on 27184 (0x6A30) and mDNS on 5353. */
    private static final String UDP6 =
            "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops\n"
                    + "  828: 00000000000000000000000000000000:6A30 00000000000000000000000000000000:0000 07 00000000:00013200 00:00000000 00000000  2000        0 128626 2 0000000000000000 0\n"
                    + " 1021: 00000000000000000000000000000000:14E9 00000000000000000000000000000000:0000 07 00000000:00000000 00:00000000 00000000  1000        0 4411 2 0000000000000000 0\n";

    @Test
    public void findsTheSocketBoundToThePort() {
        assertEquals(Collections.singleton(128626L), PortTakeover.socketInodes(UDP6, 27184));
    }

    @Test
    public void findsNothingWhenThePortIsFree() {
        assertTrue(PortTakeover.socketInodes(UDP6, 6980).isEmpty());
    }

    @Test
    public void findsEverySocketOnThePort() {
        String udp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n"
                + "   7: 00000000:6A30 00000000:0000 07 00000000:00000000 00:00000000 00000000  2000        0 7001 2\n"
                + "   9: 0100007F:6A30 00000000:0000 07 00000000:00000000 00:00000000 00000000  2000        0 7002 2\n";
        assertEquals(new HashSet<>(Arrays.asList(7001L, 7002L)), PortTakeover.socketInodes(udp, 27184));
    }

    @Test
    public void recognisesASetuServerByItsCommandLine() {
        assertTrue(PortTakeover.isServer("app_process\0/\0dev.trugamr.setu.server.Server\0udp\0"));
        assertTrue(PortTakeover.isServer("app_process\0/\0dev.trugamr.setu.server.Server\0"));
    }

    @Test
    public void leavesOtherProcessesAlone() {
        assertFalse(PortTakeover.isServer("/system/bin/mdnsd\0"));
        assertFalse(PortTakeover.isServer("app_process\0/\0dev.trugamr.setu.server.ServerTest\0"));
        assertFalse(PortTakeover.isServer(""));
    }
}
