package dev.trugamr.setu.server;

import android.app.Application;
import android.app.Instrumentation;
import android.content.Context;
import android.content.pm.ApplicationInfo;

import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;

/**
 * Sets up just enough of an ActivityThread for framework classes to work outside an app.
 * Adapted from scrcpy's Workarounds (Apache-2.0, Genymobile).
 */
final class Workarounds {

    private static final Class<?> ACTIVITY_THREAD_CLASS;
    private static final Object ACTIVITY_THREAD;

    static {
        try {
            ACTIVITY_THREAD_CLASS = Class.forName("android.app.ActivityThread");
            Constructor<?> constructor = ACTIVITY_THREAD_CLASS.getDeclaredConstructor();
            constructor.setAccessible(true);
            ACTIVITY_THREAD = constructor.newInstance();

            Field current = ACTIVITY_THREAD_CLASS.getDeclaredField("sCurrentActivityThread");
            current.setAccessible(true);
            current.set(null, ACTIVITY_THREAD);

            Field systemThread = ACTIVITY_THREAD_CLASS.getDeclaredField("mSystemThread");
            systemThread.setAccessible(true);
            systemThread.setBoolean(ACTIVITY_THREAD, true);
        } catch (ReflectiveOperationException e) {
            throw new AssertionError(e);
        }
    }

    private Workarounds() {
    }

    static void apply() {
        fillConfigurationController();
        fillAppInfo();
        fillAppContext();
    }

    static Context getSystemContext() {
        try {
            Method method = ACTIVITY_THREAD_CLASS.getDeclaredMethod("getSystemContext");
            return (Context) method.invoke(ACTIVITY_THREAD);
        } catch (Throwable t) {
            Log.info("could not get system context: " + t.getMessage());
            return null;
        }
    }

    private static void fillConfigurationController() {
        try {
            Class<?> controllerClass = Class.forName("android.app.ConfigurationController");
            Class<?> internalClass = Class.forName("android.app.ActivityThreadInternal");
            Constructor<?> constructor = controllerClass.getDeclaredConstructor(internalClass);
            constructor.setAccessible(true);
            Object controller = constructor.newInstance(ACTIVITY_THREAD);

            Field field = ACTIVITY_THREAD_CLASS.getDeclaredField("mConfigurationController");
            field.setAccessible(true);
            field.set(ACTIVITY_THREAD, controller);
        } catch (Throwable t) {
            Log.info("could not fill configuration controller: " + t.getMessage());
        }
    }

    private static void fillAppInfo() {
        try {
            Class<?> bindDataClass = Class.forName("android.app.ActivityThread$AppBindData");
            Constructor<?> constructor = bindDataClass.getDeclaredConstructor();
            constructor.setAccessible(true);
            Object bindData = constructor.newInstance();

            ApplicationInfo info = new ApplicationInfo();
            info.packageName = FakeContext.PACKAGE_NAME;
            Field appInfo = bindDataClass.getDeclaredField("appInfo");
            appInfo.setAccessible(true);
            appInfo.set(bindData, info);

            Field bound = ACTIVITY_THREAD_CLASS.getDeclaredField("mBoundApplication");
            bound.setAccessible(true);
            bound.set(ACTIVITY_THREAD, bindData);
        } catch (Throwable t) {
            Log.info("could not fill app info: " + t.getMessage());
        }
    }

    private static void fillAppContext() {
        try {
            Application app = Instrumentation.newApplication(Application.class, FakeContext.get());
            Field field = ACTIVITY_THREAD_CLASS.getDeclaredField("mInitialApplication");
            field.setAccessible(true);
            field.set(ACTIVITY_THREAD, app);
        } catch (Throwable t) {
            Log.info("could not fill app context: " + t.getMessage());
        }
    }
}
