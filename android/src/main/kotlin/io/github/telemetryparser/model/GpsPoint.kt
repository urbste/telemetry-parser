package io.github.telemetryparser.model

/**
 * GPS sample from video metadata (aligned with gpmf_android [GpsPoint] semantics).
 *
 * @property timestamp Seconds from the start of the video (same convention as GPMF tooling).
 * @property utcTime UTC time in milliseconds since epoch (from GPSU when present).
 */
data class GpsPoint(
    val latitude: Double,
    val longitude: Double,
    val altitude: Double,
    val speed2d: Double,
    val timestamp: Double,
    val utcTime: Long,
    val fix: Int,
    val precision: Int,
) {
    val speedKmh: Double get() = speed2d * 3.6
    val speedMph: Double get() = speed2d * 2.237
}
