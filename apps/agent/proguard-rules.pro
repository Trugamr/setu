# The capture server runs through app_process with this APK as its classpath, so nothing in
# the app references it; keep it whole.
-keep class dev.trugamr.setu.server.** { *; }
