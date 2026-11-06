package dev.trugamr.setu.server;

import android.os.IBinder;

import java.lang.reflect.Method;

/**
 * Wireless debugging control through the system adb service (android.debug.IAdbManager).
 * The adb shell holds MANAGE_DEBUGGING, so it can approve a Wi-Fi network for wireless
 * debugging without the on-headset confirmation dialog.
 */
final class AdbWifi {

    private AdbWifi() {
    }

    /** Marks the network with this BSSID as always allowed, then enables wireless debugging on it. */
    static void trust(String bssid) throws Exception {
        Object adb = adbManager();
        Method allow = adb.getClass().getMethod("allowWirelessDebugging", boolean.class, String.class);
        allow.invoke(adb, true, bssid);
        Log.info("adb-wifi: allowed " + bssid);
    }

    /** Revokes every adb authorization on the device, USB and wireless. adbd drops all connections. */
    static void revokeKeys() throws Exception {
        Object adb = adbManager();
        adb.getClass().getMethod("clearDebuggingKeys").invoke(adb);
        Log.info("adb-wifi: revoked all adb keys");
    }

    private static Object adbManager() throws Exception {
        Class<?> serviceManager = Class.forName("android.os.ServiceManager");
        IBinder binder = (IBinder) serviceManager.getMethod("getService", String.class).invoke(null, "adb");
        if (binder == null) {
            throw new IllegalStateException("adb service not found");
        }
        Class<?> stub = Class.forName("android.debug.IAdbManager$Stub");
        return stub.getMethod("asInterface", IBinder.class).invoke(null, binder);
    }
}
