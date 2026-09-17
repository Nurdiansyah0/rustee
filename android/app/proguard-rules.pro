# Keep JavascriptInterface annotations
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}

# Keep CoroutineWorker subclasses
-keep class * extends androidx.work.ListenableWorker {
    <init>(...);
}
