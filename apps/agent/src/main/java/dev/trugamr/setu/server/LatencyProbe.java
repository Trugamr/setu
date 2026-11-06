package dev.trugamr.setu.server;

import android.media.AudioAttributes;
import android.media.AudioFormat;
import android.media.AudioTimestamp;
import android.media.AudioTrack;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.TimeUnit;

/**
 * Measures the on-headset part of the latency: plays a click through a media AudioTrack,
 * captures it back through the same policy mix the streamer uses, and reports how long after
 * the click was written / presented the captured samples became readable.
 *
 * Other audio must be paused while this runs, since detection is a simple amplitude threshold.
 */
final class LatencyProbe {

    private static final int RATE = PlaybackCapture.SAMPLE_RATE;
    private static final int CHUNK_FRAMES = 240;
    private static final int CLICK_FRAMES = 240;
    private static final int CLICK_INTERVAL_FRAMES = RATE;
    private static final int DETECT_THRESHOLD = 4000;
    private static final int QUIET_THRESHOLD = 1000;
    private static final int QUIET_FRAMES_TO_ARM = RATE / 20;

    private static final class Click {
        final long frame;
        final long writeNanos;
        volatile long presentNanos = -1;

        Click(long frame, long writeNanos) {
            this.frame = frame;
            this.writeNanos = writeNanos;
        }
    }

    private LatencyProbe() {
    }

    static void run(int clicks, int blockFrames) throws Exception {
        PlaybackCapture capture = PlaybackCapture.open(blockFrames);
        AudioTrack track = new AudioTrack.Builder()
                .setAudioAttributes(new AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_MEDIA)
                        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                        .build())
                .setAudioFormat(new AudioFormat.Builder()
                        .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                        .setSampleRate(RATE)
                        .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
                        .build())
                .setTransferMode(AudioTrack.MODE_STREAM)
                .build();

        Log.info("probe: track buffer " + track.getBufferSizeInFrames() + " frames");

        BlockingQueue<Click> written = new ArrayBlockingQueue<>(clicks + 4);
        int totalFrames = (clicks + 2) * CLICK_INTERVAL_FRAMES;

        track.play();

        Thread writer = new Thread(new Runnable() {
            @Override
            public void run() {
                writeClicks(track, written, clicks, totalFrames);
            }
        }, "probe-writer");
        writer.start();

        long startNanos = System.nanoTime();
        int quietRun = 0;
        boolean armed = false;
        int detected = 0;
        double sumWrite = 0;
        double sumPresent = 0;
        int presentCount = 0;
        long framesRead = 0;
        int windowPeak = 0;
        long deadline = startNanos + (long) (clicks + 4) * 1_000_000_000L;

        while (detected < clicks && System.nanoTime() < deadline) {
            Capture.Block block = capture.read();
            long readNanos = System.nanoTime();
            ByteBuffer pcm = ByteBuffer.wrap(block.data, 0, block.length).order(ByteOrder.LITTLE_ENDIAN);
            // Left channel of each frame.
            for (int i = 0; i < block.length; i += PlaybackCapture.FRAME_BYTES) {
                int amp = Math.abs(pcm.getShort(i));
                windowPeak = Math.max(windowPeak, amp);
                if (++framesRead % (RATE / 2) == 0) {
                    Log.info(String.format("probe: t=%5.2fs captured %5.2fs of audio, peak %5d",
                            (readNanos - startNanos) / 1e9, framesRead / (double) RATE, windowPeak));
                    windowPeak = 0;
                }
                if (armed && amp > DETECT_THRESHOLD) {
                    armed = false;
                    quietRun = 0;
                    Click c = written.poll(1, TimeUnit.SECONDS);
                    if (c == null) {
                        Log.info("probe: detected a click that was never written (other audio playing?)");
                        continue;
                    }
                    detected++;
                    double fromWrite = (readNanos - c.writeNanos) / 1e6;
                    sumWrite += fromWrite;
                    String line = String.format("probe: click %d written at t=%5.2fs, readable at t=%5.2fs: write->readable %6.1f ms",
                            detected, (c.writeNanos - startNanos) / 1e9, (readNanos - startNanos) / 1e9, fromWrite);
                    if (c.presentNanos > 0) {
                        double fromPresent = (readNanos - c.presentNanos) / 1e6;
                        sumPresent += fromPresent;
                        presentCount++;
                        line += String.format(", presented->readable %6.1f ms", fromPresent);
                    }
                    Log.info(line);
                } else if (amp < QUIET_THRESHOLD) {
                    if (++quietRun >= QUIET_FRAMES_TO_ARM) {
                        armed = true;
                    }
                } else {
                    quietRun = 0;
                }
            }
        }

        writer.join(2000);
        track.stop();
        track.release();
        capture.close();

        if (detected == 0) {
            Log.info("probe: no clicks captured");
            return;
        }
        String summary = String.format("probe: average write->readable %.1f ms", sumWrite / detected);
        if (presentCount > 0) {
            summary += String.format(", presented->readable %.1f ms", sumPresent / presentCount);
        }
        Log.info(summary + " over " + detected + " clicks");
    }

    private static void writeClicks(AudioTrack track, BlockingQueue<Click> written, int clicks, int totalFrames) {
        short[] silence = new short[CHUNK_FRAMES * 2];
        short[] click = new short[CHUNK_FRAMES * 2];
        for (int i = 0; i < CLICK_FRAMES; i++) {
            short s = (short) (16000 * Math.sin(2 * Math.PI * 1000 * i / RATE));
            click[2 * i] = s;
            click[2 * i + 1] = s;
        }
        AudioTimestamp ts = new AudioTimestamp();
        Click pending = null;
        int clicksWritten = 0;
        for (long frame = 0; frame < totalFrames; frame += CHUNK_FRAMES) {
            boolean isClick = frame >= CLICK_INTERVAL_FRAMES && frame % CLICK_INTERVAL_FRAMES == 0
                    && clicksWritten < clicks;
            if (isClick) {
                pending = new Click(frame, System.nanoTime());
                written.offer(pending);
                clicksWritten++;
            }
            track.write(isClick ? click : silence, 0, CHUNK_FRAMES * 2);
            // Presentation time of the click, extrapolated from the track's own timestamp.
            if (pending != null && pending.presentNanos < 0 && track.getTimestamp(ts)) {
                pending.presentNanos = ts.nanoTime + (pending.frame - ts.framePosition) * 1_000_000_000L / RATE;
            }
        }
    }
}
