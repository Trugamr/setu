package dev.trugamr.setu.server;

import static org.junit.Assert.assertEquals;

import dev.trugamr.setu.MainActivityKt;

import java.util.Arrays;

import org.junit.Test;

public class SetupTest {

    @Test
    public void grantsTheSettingThenOpensTheAgentToApprove() {
        assertEquals(Arrays.asList(
                Arrays.asList("pm", "grant", "dev.trugamr.setu", "android.permission.WRITE_SECURE_SETTINGS"),
                Arrays.asList("settings", "put", "global", "adb_allowed_connection_time", "0"),
                Arrays.asList("am", "start", "-S", "-n", "dev.trugamr.setu/.MainActivity",
                        "--ez", MainActivityKt.EXTRA_APPROVE, "true")),
                Setup.steps(Setup.PACKAGE));
    }
}
