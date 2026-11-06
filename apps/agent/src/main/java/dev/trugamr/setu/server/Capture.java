package dev.trugamr.setu.server;

import dev.trugamr.setu.protocol.StreamProtocol;

import java.io.Closeable;
import java.io.IOException;

/**
 * A running capture of the headset's playback, delivered as 16-bit interleaved PCM in the
 * stream's {@code Format}.
 *
 * {@link PlaybackCapture} is the adapter on the headset; tests use a synthetic one.
 */
interface Capture extends Closeable {

    /**
     * Blocks until the next capture block lands, then returns it together with anything else
     * already captured, so a backlog leaves in one piece. The returned Block is reused by the
     * next call.
     */
    Block read() throws IOException;

    /** Releases the capture. */
    @Override
    void close();

    /** Opens captures in one format; the receiver session opens one per listening period. */
    interface Source {
        StreamProtocol.Format format();

        Capture open() throws IOException;
    }

    /** PCM from one {@link #read()}. */
    final class Block {
        byte[] data;
        int length;
        /** Index of the first frame in {@code data}, counted from the start of the capture. */
        long framePosition;
    }
}
