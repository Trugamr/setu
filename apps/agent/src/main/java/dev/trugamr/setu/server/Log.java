package dev.trugamr.setu.server;

/** The server's log: stdout, which the launching shell redirects to a file. */
final class Log {

    private Log() {
    }

    static void info(String message) {
        System.out.println("[setu] " + message);
    }
}
