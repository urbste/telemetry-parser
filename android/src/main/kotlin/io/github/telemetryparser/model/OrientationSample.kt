package io.github.telemetryparser.model

/**
 * CORI / IORI / combined orientation at [timestampNs] from clip start.
 * Components follow export order `(-x, z, y, w)` from the internal `(w,x,y,z)` quaternion
 * (i.e. labels a,b,c,d → -b, d, c, a).
 */
data class OrientationSample(
    val timestampNs: Long,
    val w: Double,
    val x: Double,
    val y: Double,
    val z: Double,
)
