package io.github.telemetryparser.model

/**
 * Accelerometer or gyroscope sample (or gravity vector).
 *
 * Accelerometer: m/s². Gyroscope: rad/s. Gravity: m/s² (direction); gravity uses component
 * order **(c, a, b)** after internal **(a, b, c)**.
 *
 * @property timestampNs Time from clip start in nanoseconds.
 */
data class SensorSample(
    val timestampNs: Long,
    val x: Float,
    val y: Float,
    val z: Float,
) {
    val magnitude: Double
        get() = kotlin.math.sqrt((x * x + y * y + z * z).toDouble())
}
