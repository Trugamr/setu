package dev.trugamr.setu.server;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

import dev.trugamr.setu.protocol.StreamProtocol;

import java.io.IOException;
import dev.trugamr.setu.server.Transport.Datagram;
import java.net.InetSocketAddress;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.Collections;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;

import org.junit.After;
import org.junit.Test;

public class ReceiverSessionTest {

    private static final int BLOCK_FRAMES = 96;
    private static final StreamProtocol.Format FORMAT = new StreamProtocol.Format(48000, 2, 2, BLOCK_FRAMES);
    /** From the spec: a 16-byte AUDIO header, at most 340 frames per datagram. */
    private static final int AUDIO_HEADER_BYTES = 16;
    private static final int MAX_AUDIO_FRAMES = 340;
    private static final InetSocketAddress PC = new InetSocketAddress("10.0.0.2", 5000);
    private static final InetSocketAddress OTHER_PC = new InetSocketAddress("10.0.0.3", 5000);
    private static final InetSocketAddress AGENT = new InetSocketAddress("127.0.0.1", 6000);
    private static final InetSocketAddress VBAN_PC = new InetSocketAddress("10.0.0.4", 6980);
    private static final StreamProtocol.VbanTarget VBAN = new StreamProtocol.VbanTarget(VBAN_PC, "setu");
    private static final int VBAN_HEADER_BYTES = 28;

    private final FakeTransport transport = new FakeTransport();
    private final AtomicLong clock = new AtomicLong(1_000_000);
    private final SyntheticCaptures captures = new SyntheticCaptures();
    private Thread thread;
    private volatile Throwable thrown;

    @After
    public void stopSession() throws Exception {
        if (thread != null && thread.isAlive()) {
            transport.deliver(StreamProtocol.empty(StreamProtocol.STOP), AGENT);
            thread.join(3000);
            assertFalse("session did not stop", thread.isAlive());
        }
    }

    @Test
    public void helloGetsFormatThenAudio() throws Exception {
        start();
        transport.deliver(StreamProtocol.hello("studio-pc"), PC);

        Datagram format = transport.sent.poll(2, TimeUnit.SECONDS);
        assertNotNull(format);
        assertEquals("FORMAT comes before any AUDIO", StreamProtocol.FORMAT, StreamProtocol.type(format.data, format.data.length));
        assertEquals(PC, format.from);
        assertEquals(FORMAT, StreamProtocol.parseFormat(format.data, format.data.length));

        for (int i = 0; i < 3; i++) {
            Datagram audio = transport.next(StreamProtocol.AUDIO);
            assertEquals(PC, audio.from);
            assertEquals(i, StreamProtocol.audioSequence(audio.data, audio.data.length));
            assertEquals((long) i * BLOCK_FRAMES, StreamProtocol.audioFramePosition(audio.data, audio.data.length));
            assertEquals(AUDIO_HEADER_BYTES + BLOCK_FRAMES * FORMAT.frameBytes(), audio.data.length);
        }
    }

    @Test
    public void newestReceiverGetsTheStream() throws Exception {
        start();
        transport.deliver(StreamProtocol.hello("a"), PC);
        assertEquals(PC, transport.next(StreamProtocol.AUDIO).from);

        transport.deliver(StreamProtocol.hello("b"), OTHER_PC);
        Datagram audio;
        do {
            audio = transport.next(StreamProtocol.AUDIO);
        } while (audio.from.equals(PC));
        assertEquals(OTHER_PC, audio.from);
        for (int i = 0; i < 5; i++) {
            assertEquals(OTHER_PC, transport.next(StreamProtocol.AUDIO).from);
        }
    }

    @Test
    public void quietReceiverReleasesTheCapture() throws Exception {
        start();
        transport.deliver(StreamProtocol.hello("studio-pc"), PC);
        transport.next(StreamProtocol.AUDIO);
        assertEquals(1, captures.opened.get());

        clock.addAndGet(StreamProtocol.STALE_RECEIVER_MS + 1);
        awaitTrue(() -> captures.closed.get() == 1);
        transport.sent.clear();
        Thread.sleep(50);
        assertTrue("no audio after release", transport.sent.isEmpty());

        // A new hello opens a fresh capture whose sequence starts over.
        transport.deliver(StreamProtocol.hello("studio-pc"), PC);
        Datagram audio = transport.next(StreamProtocol.AUDIO);
        assertEquals(0, StreamProtocol.audioSequence(audio.data, audio.data.length));
        assertEquals(2, captures.opened.get());
    }

    @Test
    public void largeBlocksAreSplitToFitTheMtu() throws Exception {
        captures.readFrames = 1000;
        start();
        transport.deliver(StreamProtocol.hello("studio-pc"), PC);

        int[] frames = {MAX_AUDIO_FRAMES, MAX_AUDIO_FRAMES, 1000 - 2 * MAX_AUDIO_FRAMES};
        long position = 0;
        for (int i = 0; i < frames.length; i++) {
            Datagram audio = transport.next(StreamProtocol.AUDIO);
            assertEquals(i, StreamProtocol.audioSequence(audio.data, audio.data.length));
            assertEquals(position, StreamProtocol.audioFramePosition(audio.data, audio.data.length));
            assertEquals(AUDIO_HEADER_BYTES + frames[i] * FORMAT.frameBytes(), audio.data.length);
            position += frames[i];
        }
    }

    @Test
    public void statusListsTheReceiverForTheAgent() throws Exception {
        start();
        transport.deliver(StreamProtocol.empty(StreamProtocol.STATUS_REQUEST), AGENT);
        Datagram idle = transport.next(StreamProtocol.STATUS);
        assertEquals(AGENT, idle.from);
        assertEquals(new StreamProtocol.Status(BLOCK_FRAMES, Collections.emptyList(), null),
                StreamProtocol.parseStatus(idle.data, idle.data.length));

        transport.deliver(StreamProtocol.hello("studio-pc"), PC);
        transport.next(StreamProtocol.AUDIO);
        transport.deliver(StreamProtocol.empty(StreamProtocol.STATUS_REQUEST), AGENT);
        Datagram busy = transport.next(StreamProtocol.STATUS);
        assertEquals(Collections.singletonList(new StreamProtocol.Receiver("studio-pc", "10.0.0.2:5000")),
                StreamProtocol.parseStatus(busy.data, busy.data.length).receivers);
    }

    @Test
    public void statusAndStopAreIgnoredFromTheNetwork() throws Exception {
        start();
        transport.deliver(StreamProtocol.empty(StreamProtocol.STATUS_REQUEST), PC);
        transport.deliver(StreamProtocol.empty(StreamProtocol.STOP), PC);
        // Datagrams are handled in order, so the agent's reply means the PC's were handled.
        transport.deliver(StreamProtocol.empty(StreamProtocol.STATUS_REQUEST), AGENT);
        assertEquals(AGENT, transport.next(StreamProtocol.STATUS).from);
        assertTrue(thread.isAlive());
    }

    @Test
    public void stopFromTheAgentEndsTheSession() throws Exception {
        start();
        transport.deliver(StreamProtocol.hello("studio-pc"), PC);
        transport.next(StreamProtocol.AUDIO);

        transport.deliver(StreamProtocol.empty(StreamProtocol.STOP), AGENT);
        thread.join(2000);
        assertFalse(thread.isAlive());
        assertNull(thrown);
        assertEquals(1, captures.closed.get());
    }

    @Test
    public void receiverThatCannotBeRepliedToIsIgnored() throws Exception {
        transport.failSendType = StreamProtocol.FORMAT;
        start();
        transport.deliver(StreamProtocol.hello("studio-pc"), PC);
        assertTrue(status().receivers.isEmpty());
        assertTrue(thread.isAlive());
        assertEquals(0, captures.opened.get());
    }

    @Test
    public void unreachableReceiverIsDroppedUntilItSaysHelloAgain() throws Exception {
        transport.failSendType = StreamProtocol.AUDIO;
        start();
        transport.deliver(StreamProtocol.hello("studio-pc"), PC);
        awaitTrue(() -> captures.closed.get() == 1);
        assertTrue(status().receivers.isEmpty());
        assertTrue(thread.isAlive());

        transport.failSendType = 0;
        transport.deliver(StreamProtocol.hello("studio-pc"), PC);
        assertEquals(PC, transport.next(StreamProtocol.AUDIO).from);
    }

    @Test
    public void stopReturnsEvenWhenACaptureReadHangs() throws Exception {
        captures.hangRead = true;
        start();
        transport.deliver(StreamProtocol.hello("studio-pc"), PC);
        awaitTrue(() -> captures.opened.get() == 1);

        transport.deliver(StreamProtocol.empty(StreamProtocol.STOP), AGENT);
        thread.join(3000);
        assertFalse(thread.isAlive());
        assertNull(thrown);
    }

    @Test
    public void captureFailureEndsTheSessionWithTheError() throws Exception {
        captures.failOpen = true;
        start();
        transport.deliver(StreamProtocol.hello("studio-pc"), PC);
        thread.join(2000);
        assertFalse(thread.isAlive());
        assertTrue(thrown instanceof IOException);
    }

    @Test
    public void vbanStreamsWithoutAHello() throws Exception {
        start();
        transport.deliver(StreamProtocol.setVban(VBAN), AGENT);
        for (int i = 0; i < 3; i++) {
            Datagram packet = transport.nextTo(VBAN_PC);
            assertEquals("VBAN", new String(packet.data, 0, 4, StandardCharsets.US_ASCII));
            assertEquals("frames - 1", BLOCK_FRAMES - 1, packet.data[5] & 0xff);
            assertEquals("setu", new String(packet.data, 8, 4, StandardCharsets.US_ASCII));
            assertEquals(i, vbanCounter(packet));
            assertEquals(VBAN_HEADER_BYTES + BLOCK_FRAMES * FORMAT.frameBytes(), packet.data.length);
        }
        assertEquals(VBAN, status().vban);
    }

    @Test
    public void vbanAndASetuReceiverShareOneCapture() throws Exception {
        start();
        transport.deliver(StreamProtocol.setVban(VBAN), AGENT);
        transport.nextTo(VBAN_PC);
        transport.deliver(StreamProtocol.hello("studio-pc"), PC);
        assertEquals(PC, transport.next(StreamProtocol.AUDIO).from);
        transport.nextTo(VBAN_PC);
        assertEquals(1, captures.opened.get());

        // The Setu receiver goes quiet; VBAN keeps the capture.
        clock.addAndGet(StreamProtocol.STALE_RECEIVER_MS + 1);
        transport.nextTo(VBAN_PC);
        transport.sent.clear();
        for (int i = 0; i < 5; i++) {
            assertEquals(VBAN_PC, transport.nextTo(VBAN_PC).from);
        }
        assertTrue(status().receivers.isEmpty());
        assertEquals(0, captures.closed.get());
    }

    @Test
    public void turningVbanOffReleasesTheCapture() throws Exception {
        start();
        transport.deliver(StreamProtocol.setVban(VBAN), AGENT);
        transport.nextTo(VBAN_PC);
        transport.deliver(StreamProtocol.setVban(null), AGENT);
        awaitTrue(() -> captures.closed.get() == 1);
        assertNull(status().vban);
    }

    @Test
    public void unreachableVbanTargetIsKept() throws Exception {
        transport.failSendTo = VBAN_PC;
        start();
        transport.deliver(StreamProtocol.setVban(VBAN), AGENT);
        awaitTrue(() -> captures.opened.get() == 1);
        Thread.sleep(20);
        assertEquals(VBAN, status().vban);
        assertEquals(0, captures.closed.get());

        transport.failSendTo = null;
        assertEquals(VBAN_PC, transport.nextTo(VBAN_PC).from);
    }

    @Test
    public void changingTheVbanTargetMovesTheStream() throws Exception {
        start();
        transport.deliver(StreamProtocol.setVban(VBAN), AGENT);
        transport.nextTo(VBAN_PC);
        StreamProtocol.VbanTarget other = new StreamProtocol.VbanTarget(OTHER_PC, "Left eye");
        transport.deliver(StreamProtocol.setVban(other), AGENT);
        Datagram packet = transport.nextTo(OTHER_PC);
        assertEquals("Left eye", new String(packet.data, 8, 8, StandardCharsets.US_ASCII));
        assertEquals(other, status().vban);
    }

    @Test
    public void largeBlocksAreSplitToFitVban() throws Exception {
        captures.readFrames = 600;
        start();
        transport.deliver(StreamProtocol.setVban(VBAN), AGENT);
        int[] frames = {256, 256, 600 - 2 * 256};
        for (int i = 0; i < frames.length; i++) {
            Datagram packet = transport.nextTo(VBAN_PC);
            assertEquals(i, vbanCounter(packet));
            assertEquals(VBAN_HEADER_BYTES + frames[i] * FORMAT.frameBytes(), packet.data.length);
        }
    }

    @Test
    public void setVbanIsIgnoredFromTheNetwork() throws Exception {
        start();
        transport.deliver(StreamProtocol.setVban(VBAN), PC);
        assertNull(status().vban);
        assertEquals(0, captures.opened.get());
    }

    private static int vbanCounter(Datagram packet) {
        return ByteBuffer.wrap(packet.data, 24, 4).order(ByteOrder.LITTLE_ENDIAN).getInt();
    }

    private StreamProtocol.Status status() throws InterruptedException {
        transport.deliver(StreamProtocol.empty(StreamProtocol.STATUS_REQUEST), AGENT);
        Datagram reply = transport.next(StreamProtocol.STATUS);
        return StreamProtocol.parseStatus(reply.data, reply.data.length);
    }

    private void start() {
        ReceiverSession session = new ReceiverSession(transport, captures, clock::get);
        thread = new Thread(() -> {
            try {
                session.run();
            } catch (Throwable t) {
                thrown = t;
            }
        });
        thread.start();
    }

    private static void awaitTrue(java.util.function.BooleanSupplier condition) throws InterruptedException {
        long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(2);
        while (!condition.getAsBoolean()) {
            if (System.nanoTime() > deadline) {
                fail("condition not met in 2 s");
            }
            Thread.sleep(5);
        }
    }

    /** Records sent datagrams as {@link Transport.Datagram}s whose {@code from} is the destination. */
    private static final class FakeTransport implements Transport {
        final BlockingQueue<Datagram> inbound = new LinkedBlockingQueue<>();
        final BlockingQueue<Datagram> sent = new LinkedBlockingQueue<>();
        /** Sends of this Setu message type throw, as if the destination were unreachable; 0 is none. */
        volatile int failSendType = 0;
        /** Sends to this address throw. */
        volatile InetSocketAddress failSendTo;

        void deliver(byte[] data, InetSocketAddress from) {
            inbound.add(new Datagram(data, from));
        }

        /** The next sent message of this type, skipping others; fails after 2 s. */
        Datagram next(int type) throws InterruptedException {
            long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(2);
            while (true) {
                long left = deadline - System.nanoTime();
                Datagram s = sent.poll(Math.max(0, left), TimeUnit.NANOSECONDS);
                assertNotNull("no message of type " + type + " sent", s);
                if (StreamProtocol.type(s.data, s.data.length) == type) {
                    return s;
                }
            }
        }

        @Override
        public Datagram receive(int timeoutMs) throws IOException {
            try {
                return inbound.poll(timeoutMs, TimeUnit.MILLISECONDS);
            } catch (InterruptedException e) {
                throw new IOException(e);
            }
        }

        /** The next datagram sent to {@code to}, skipping others; fails after 2 s. */
        Datagram nextTo(InetSocketAddress to) throws InterruptedException {
            long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(2);
            while (true) {
                long left = deadline - System.nanoTime();
                Datagram s = sent.poll(Math.max(0, left), TimeUnit.NANOSECONDS);
                assertNotNull("nothing sent to " + to, s);
                if (s.from.equals(to)) {
                    return s;
                }
            }
        }

        @Override
        public void send(byte[] data, int length, InetSocketAddress to) throws IOException {
            if (to.equals(failSendTo) || StreamProtocol.type(data, length) == failSendType) {
                throw new IOException("network unreachable");
            }
            sent.add(new Datagram(Arrays.copyOf(data, length), to));
        }
    }

    /** Delivers {@code readFrames} of silence every millisecond. Set fields before {@link #start}. */
    private static final class SyntheticCaptures implements Capture.Source {
        final AtomicInteger opened = new AtomicInteger();
        final AtomicInteger closed = new AtomicInteger();
        int readFrames = BLOCK_FRAMES;
        boolean failOpen;
        /** Reads block forever, ignoring interrupts, like a capture that never delivers. */
        boolean hangRead;

        @Override
        public StreamProtocol.Format format() {
            return FORMAT;
        }

        @Override
        public Capture open() throws IOException {
            if (failOpen) {
                throw new IOException("capture unavailable");
            }
            opened.incrementAndGet();
            int frames = readFrames;
            return new Capture() {
                final Block block = new Block();
                long position;

                {
                    block.data = new byte[frames * FORMAT.frameBytes()];
                    block.length = block.data.length;
                }

                @Override
                public Block read() throws IOException {
                    while (hangRead) {
                        try {
                            Thread.sleep(1000);
                        } catch (InterruptedException ignored) {
                            // Keeps hanging.
                        }
                    }
                    try {
                        Thread.sleep(1);
                    } catch (InterruptedException e) {
                        throw new IOException(e);
                    }
                    block.framePosition = position;
                    position += frames;
                    return block;
                }

                @Override
                public void close() {
                    closed.incrementAndGet();
                }
            };
        }
    }
}
