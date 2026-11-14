# Framework instrumentation calls this test-variant boundary by reflection.
-keep class com.codync.android.ReleaseWidgetSmoke {
    public static void run(android.app.Instrumentation);
}
