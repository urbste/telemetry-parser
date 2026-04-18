# Keep JNI native methods
-keepclasseswithmembernames class * {
    native <methods>;
}

# Keep model classes (used by JNI)
-keep class io.github.telemetryparser.model.** { *; }
