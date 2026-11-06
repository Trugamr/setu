package dev.trugamr.setu.server;

import android.content.AttributionSource;
import android.content.Context;
import android.content.ContextWrapper;

/**
 * A Context that identifies as com.android.shell, so system services accept calls from a
 * process started with app_process. Adapted from scrcpy's FakeContext (Apache-2.0, Genymobile).
 */
final class FakeContext extends ContextWrapper {

    static final String PACKAGE_NAME = "com.android.shell";
    private static final int SHELL_UID = 2000;

    private static final FakeContext INSTANCE = new FakeContext();

    static FakeContext get() {
        return INSTANCE;
    }

    private FakeContext() {
        super(Workarounds.getSystemContext());
    }

    @Override
    public String getPackageName() {
        return PACKAGE_NAME;
    }

    @Override
    public String getOpPackageName() {
        return PACKAGE_NAME;
    }

    @Override
    public AttributionSource getAttributionSource() {
        return new AttributionSource.Builder(SHELL_UID).setPackageName(PACKAGE_NAME).build();
    }

    @SuppressWarnings("unused")
    public int getDeviceId() {
        return 0;
    }

    @Override
    public Context getApplicationContext() {
        return this;
    }
}
