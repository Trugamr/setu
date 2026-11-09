package dev.trugamr.setu.server;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.List;

/**
 * The one-time setup every installer runs, as the shell user, once the APK is installed:
 *
 *   CLASSPATH=$(pm path dev.trugamr.setu | cut -d: -f2) app_process / dev.trugamr.setu.server.Server setup
 *
 * It grants the agent WRITE_SECURE_SETTINGS, so the agent can turn wireless debugging on, makes
 * allowed adb keys never expire, and opens the agent to approve its key. The agent then waits
 * for adbd's TCP port, which only the PC can open: the installer's last step is `adb tcpip 5555`.
 * When the headset has allowed the key, the agent returns adbd to USB, so an installer knows
 * setup finished when {@code service.adb.tcp.port} reads 0 again.
 *
 * Prints {@link #DONE} as its last line when every step succeeded.
 */
final class Setup {

    static final String PACKAGE = "dev.trugamr.setu";
    static final String DONE = "setup: done";

    private Setup() {
    }

    /** The commands, in order. */
    static List<List<String>> steps(String packageName) {
        return Arrays.asList(
                Arrays.asList("pm", "grant", packageName, "android.permission.WRITE_SECURE_SETTINGS"),
                // An allowed key never expires (the default is 7 days).
                Arrays.asList("settings", "put", "global", "adb_allowed_connection_time", "0"),
                // -S restarts the agent, so it reads the extra even when it was open.
                Arrays.asList("am", "start", "-S", "-n", packageName + "/.MainActivity",
                        "--ez", packageName + ".APPROVE", "true"));
    }

    static void run() throws IOException, InterruptedException {
        for (List<String> step : steps(PACKAGE)) {
            String command = String.join(" ", step);
            Process process = new ProcessBuilder(step).redirectErrorStream(true).start();
            String output = read(process.getInputStream()).trim();
            int exit = process.waitFor();
            // `am start` reports a bad component on its output and still exits 0.
            if (exit != 0 || output.contains("Error:")) {
                System.out.println("setup: failed: " + command + (output.isEmpty() ? "" : ": " + output));
                System.exit(1);
            }
            System.out.println("setup: " + command);
        }
        System.out.println(DONE);
    }

    // InputStream.readAllBytes needs API 33.
    private static String read(InputStream in) throws IOException {
        try (in) {
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            byte[] buffer = new byte[4096];
            for (int n; (n = in.read(buffer)) > 0; ) {
                out.write(buffer, 0, n);
            }
            return out.toString(StandardCharsets.UTF_8.name());
        }
    }
}
