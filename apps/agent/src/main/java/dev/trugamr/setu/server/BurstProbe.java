package dev.trugamr.setu.server;

import java.io.IOException;

/**
 * Reports how the capture path delivers audio: the gap between reads that had to wait for a
 * block, and how many frames each wake-up carries. The PC-side jitter buffer must cover the
 * longest gap.
 */
final class BurstProbe {

    private BurstProbe() {
    }

    static void run(int seconds, int blockFrames) throws IOException {
        try (PlaybackCapture capture = PlaybackCapture.open(blockFrames)) {
            long end = System.nanoTime() + seconds * 1_000_000_000L;
            long last = System.nanoTime();
            int wakeUps = 0;
            double maxGapMs = 0;
            double sumGapMs = 0;
            long maxFrames = 0;
            StringBuilder sample = new StringBuilder();
            while (System.nanoTime() < end) {
                Capture.Block block = capture.read();
                long now = System.nanoTime();
                double gapMs = (now - last) / 1e6;
                last = now;
                long frames = block.length / PlaybackCapture.FRAME_BYTES;
                if (wakeUps > 0) {
                    if (wakeUps <= 12) {
                        sample.append(String.format(" %.1fms/%d", gapMs, frames));
                    }
                    maxGapMs = Math.max(maxGapMs, gapMs);
                    sumGapMs += gapMs;
                }
                maxFrames = Math.max(maxFrames, frames);
                wakeUps++;
            }
            Log.info(String.format("bursts: %d wake-ups in %ds, avg gap %.1f ms, max gap %.1f ms, max frames per wake-up %d",
                    wakeUps, seconds, sumGapMs / Math.max(1, wakeUps - 1), maxGapMs, maxFrames));
            Log.info("bursts: first wake-ups (gap/frames):" + sample);
        }
    }
}
