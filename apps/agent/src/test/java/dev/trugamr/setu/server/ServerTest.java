package dev.trugamr.setu.server;

import static org.junit.Assert.assertEquals;

import dev.trugamr.setu.protocol.StreamProtocol;

import java.net.InetSocketAddress;

import org.junit.Test;

public class ServerTest {

    @Test
    public void vbanTargetDefaultsToPort6980AndStreamSetu() throws Exception {
        assertEquals(new StreamProtocol.VbanTarget(new InetSocketAddress("192.168.1.20", 6980), "setu"),
                Server.parseVbanTarget("192.168.1.20"));
    }

    @Test
    public void vbanTargetTakesAPortAndAName() throws Exception {
        assertEquals(new StreamProtocol.VbanTarget(new InetSocketAddress("192.168.1.20", 6981), "Stream1"),
                Server.parseVbanTarget("192.168.1.20:6981/Stream1"));
        assertEquals(new StreamProtocol.VbanTarget(new InetSocketAddress("fe80::1", 6980), "setu"),
                Server.parseVbanTarget("[fe80::1]:6980"));
        assertEquals(new StreamProtocol.VbanTarget(new InetSocketAddress("fe80::1", 6980), "setu"),
                Server.parseVbanTarget("fe80::1"));
    }
}
