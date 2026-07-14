# Applied to app when this AAR is a dependency. JNI uses exact class/method names from Rust.
-keepclasseswithmembernames class io.github.telemetryparser.TelemetryParser {
    native <methods>;
}
-keep class io.github.telemetryparser.TelemetryParser { *; }
-keep class io.github.telemetryparser.model.** { *; }
