package dev.trugamr.setu.server;

import android.content.Context;
import android.media.AudioAttributes;
import android.media.AudioFormat;
import android.media.AudioManager;
import android.media.AudioRecord;
import android.os.Process;

import dev.trugamr.setu.protocol.StreamProtocol;

import java.io.IOException;
import java.lang.reflect.Method;

/**
 * The {@link Capture} adapter on the headset: registers a dynamic audio policy mix that loops
 * playback back to an AudioRecord. Adapted from scrcpy's AudioPlaybackCapture (Apache-2.0,
 * Genymobile).
 *
 * Horizon OS routes media/game audio through render-only policy mixes to AUDIO_DEVICE_OUT_BUS
 * devices. Android treats those as the primary mix, and a LOOP_BACK_RENDER mix as a secondary
 * output added on top of it, so capture coexists with Quest's routing instead of competing.
 * (A LOOP_BACK mix competes for the primary slot and loses.)
 *
 * Horizon OS sizes its capture engine's block from the AudioRecord buffer, so the block size
 * sets the capture period directly (96 frames = 2 ms; AudioPolicy.createAudioRecordSink() asks
 * for 1920 frames, 40 ms).
 */
final class PlaybackCapture implements Capture {

    static final int SAMPLE_RATE = 48000;
    private static final int CHANNELS = 2;
    private static final int BYTES_PER_SAMPLE = 2;
    static final int FRAME_BYTES = CHANNELS * BYTES_PER_SAMPLE;
    /** The read buffer holds this many record buffers, so a backlog drains in one read. */
    private static final int READ_BUFFER_BLOCKS = 4;
    /** Each read first blocks for this many frames, to wake as soon as a block lands. */
    private static final int WAKE_FRAMES = 16;

    private static final int[] USAGES = {
            AudioAttributes.USAGE_UNKNOWN,
            AudioAttributes.USAGE_MEDIA,
            AudioAttributes.USAGE_GAME,
            AudioAttributes.USAGE_VOICE_COMMUNICATION,
            AudioAttributes.USAGE_ALARM,
            AudioAttributes.USAGE_NOTIFICATION,
            AudioAttributes.USAGE_NOTIFICATION_EVENT,
            AudioAttributes.USAGE_ASSISTANCE_SONIFICATION,
            AudioAttributes.USAGE_ASSISTANT,
    };

    private final AudioRecord recorder;
    private final Object policy;
    private final int wakeBytes;
    private final Block block = new Block();
    private long framesRead;

    private PlaybackCapture(AudioRecord recorder, Object policy, int blockFrames) {
        this.recorder = recorder;
        this.policy = policy;
        this.wakeBytes = Math.min(blockFrames, WAKE_FRAMES) * FRAME_BYTES;
        block.data = new byte[Math.max(recorder.getBufferSizeInFrames(), blockFrames) * READ_BUFFER_BLOCKS * FRAME_BYTES];
    }

    static Capture.Source source(int blockFrames) {
        StreamProtocol.Format format = new StreamProtocol.Format(SAMPLE_RATE, CHANNELS, BYTES_PER_SAMPLE, blockFrames);
        return new Capture.Source() {
            @Override
            public StreamProtocol.Format format() {
                return format;
            }

            @Override
            public Capture open() throws IOException {
                return PlaybackCapture.open(blockFrames);
            }
        };
    }

    /**
     * Starts capturing with capture blocks of {@code blockFrames}. The calling thread is raised
     * to urgent audio priority, so it should be the thread that reads.
     */
    static PlaybackCapture open(int blockFrames) throws IOException {
        Object[] policyAndMix = registerPolicy();
        Object policy = policyAndMix[0];
        AudioRecord recorder = null;
        try {
            recorder = createSink(policyAndMix[1], blockFrames);
            try {
                Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO);
            } catch (SecurityException | IllegalArgumentException e) {
                Log.info("could not raise thread priority: " + e.getMessage());
            }
            recorder.startRecording();
            Log.info("capturing (record buffer " + recorder.getBufferSizeInFrames() + " frames)");
            return new PlaybackCapture(recorder, policy, blockFrames);
        } catch (IOException | RuntimeException e) {
            if (recorder != null) {
                recorder.release();
            }
            unregisterPolicy(policy);
            throw e instanceof IOException ? (IOException) e : new IOException("could not start playback capture", e);
        }
    }

    @Override
    public Block read() throws IOException {
        int read = recorder.read(block.data, 0, wakeBytes, AudioRecord.READ_BLOCKING);
        if (read < 0) {
            throw new IOException("AudioRecord.read() returned " + read);
        }
        int more = recorder.read(block.data, read, block.data.length - read, AudioRecord.READ_NON_BLOCKING);
        if (more > 0) {
            read += more;
        }
        block.length = read;
        block.framePosition = framesRead;
        framesRead += read / FRAME_BYTES;
        return block;
    }

    /** Releases the recorder and unregisters the policy, so repeated captures do not pile up mixes. */
    @Override
    public void close() {
        recorder.release();
        unregisterPolicy(policy);
    }

    private static void unregisterPolicy(Object policy) {
        try {
            Method unregister = AudioManager.class.getDeclaredMethod("unregisterAudioPolicyAsyncStatic",
                    Class.forName("android.media.audiopolicy.AudioPolicy"));
            unregister.setAccessible(true);
            unregister.invoke(null, policy);
        } catch (ReflectiveOperationException | RuntimeException e) {
            Log.info("could not unregister the capture policy: " + e);
        }
    }

    /**
     * Registers a policy with one LOOP_BACK_RENDER mix that matches every playback usage.
     * Returns the policy and the mix.
     */
    private static Object[] registerPolicy() throws IOException {
        try {
            Class<?> ruleClass = Class.forName("android.media.audiopolicy.AudioMixingRule");
            Class<?> ruleBuilderClass = Class.forName("android.media.audiopolicy.AudioMixingRule$Builder");
            Object ruleBuilder = ruleBuilderClass.getConstructor().newInstance();

            int mixRolePlayers = ruleClass.getField("MIX_ROLE_PLAYERS").getInt(null);
            ruleBuilderClass.getMethod("setTargetMixRole", int.class).invoke(ruleBuilder, mixRolePlayers);

            int matchUsage = ruleClass.getField("RULE_MATCH_ATTRIBUTE_USAGE").getInt(null);
            Method addMixRule = ruleBuilderClass.getMethod("addMixRule", int.class, Object.class);
            for (int usage : USAGES) {
                addMixRule.invoke(ruleBuilder, matchUsage, new AudioAttributes.Builder().setUsage(usage).build());
            }
            ruleBuilderClass.getMethod("voiceCommunicationCaptureAllowed", boolean.class).invoke(ruleBuilder, true);
            Object rule = ruleBuilderClass.getMethod("build").invoke(ruleBuilder);

            Class<?> mixClass = Class.forName("android.media.audiopolicy.AudioMix");
            Class<?> mixBuilderClass = Class.forName("android.media.audiopolicy.AudioMix$Builder");
            Object mixBuilder = mixBuilderClass.getConstructor(ruleClass).newInstance(rule);
            mixBuilderClass.getMethod("setFormat", AudioFormat.class).invoke(mixBuilder, audioFormat());
            int loopBackRender = mixClass.getField("ROUTE_FLAG_LOOP_BACK_RENDER").getInt(null);
            mixBuilderClass.getMethod("setRouteFlags", int.class).invoke(mixBuilder, loopBackRender);
            Object mix = mixBuilderClass.getMethod("build").invoke(mixBuilder);

            Class<?> policyClass = Class.forName("android.media.audiopolicy.AudioPolicy");
            Class<?> policyBuilderClass = Class.forName("android.media.audiopolicy.AudioPolicy$Builder");
            Object policyBuilder = policyBuilderClass.getConstructor(Context.class).newInstance(ShellRuntime.context());
            policyBuilderClass.getMethod("addMix", mixClass).invoke(policyBuilder, mix);
            Object policy = policyBuilderClass.getMethod("build").invoke(policyBuilder);

            Method register = AudioManager.class.getDeclaredMethod("registerAudioPolicyStatic", policyClass);
            register.setAccessible(true);
            int result = (int) register.invoke(null, policy);
            if (result != 0) {
                throw new IOException("registerAudioPolicyStatic() returned " + result);
            }
            return new Object[] {policy, mix};
        } catch (ReflectiveOperationException e) {
            throw new IOException("could not set up playback capture", e);
        }
    }

    /** Equivalent of AudioPolicy.createAudioRecordSink() with a caller-chosen buffer size. */
    private static AudioRecord createSink(Object mix, int bufferFrames) throws IOException {
        try {
            return newSink(mix, bufferFrames);
        } catch (ReflectiveOperationException e) {
            throw new IOException("could not create the capture AudioRecord", e);
        }
    }

    private static AudioRecord newSink(Object mix, int bufferFrames) throws ReflectiveOperationException {
        Class<?> mixClass = Class.forName("android.media.audiopolicy.AudioMix");
        String registration = (String) mixClass.getMethod("getRegistration").invoke(mix);
        AudioAttributes.Builder builder = new AudioAttributes.Builder();
        Method setPreset = AudioAttributes.Builder.class.getMethod("setInternalCapturePreset", int.class);
        Method addTag = AudioAttributes.Builder.class.getMethod("addTag", String.class);
        // MediaRecorder.AudioSource.REMOTE_SUBMIX. (AudioPolicy uses VOICE_COMMUNICATION when the
        // rule allows voice capture, but on Horizon OS that sink never receives audio.)
        setPreset.invoke(builder, 8);
        addTag.invoke(builder, "addr=" + registration);
        addTag.invoke(builder, "fixedVolume");
        AudioAttributes attributes = builder.build();

        return AudioRecord.class
                .getConstructor(AudioAttributes.class, AudioFormat.class, int.class, int.class)
                .newInstance(attributes, audioFormat(), bufferFrames * FRAME_BYTES, 0 /* AUDIO_SESSION_ID_GENERATE */);
    }

    private static AudioFormat audioFormat() {
        return new AudioFormat.Builder()
                .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                .setSampleRate(SAMPLE_RATE)
                .setChannelMask(AudioFormat.CHANNEL_IN_STEREO)
                .build();
    }
}
