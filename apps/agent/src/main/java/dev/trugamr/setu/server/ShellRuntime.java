package dev.trugamr.setu.server;

import android.content.Context;
import android.os.Looper;

/**
 * Makes framework classes usable in a process started with {@code app_process} as the shell
 * user. {@link #init()} must run first in {@code main}; after it, {@link #context()} is a Context
 * that system services accept as com.android.shell.
 *
 * The work is done by {@link Workarounds} and {@link FakeContext}, kept close to their scrcpy
 * originals so upstream fixes stay easy to port.
 */
final class ShellRuntime {

    private ShellRuntime() {
    }

    // prepareMainLooper() is deprecated for apps, whose main looper the system creates; this
    // process has no app, so it has to make its own.
    @SuppressWarnings("deprecation")
    static void init() {
        Looper.prepareMainLooper();
        Workarounds.apply();
    }

    static Context context() {
        return FakeContext.get();
    }
}
