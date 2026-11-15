# Framework instrumentation calls these test-variant boundaries by reflection.
-keep class com.codync.android.ReleaseWidgetSmoke {
    public static void run(android.app.Instrumentation);
}
-keep class com.codync.android.ReleaseWebRtcSmoke {
    public static void run(android.app.Instrumentation);
}
