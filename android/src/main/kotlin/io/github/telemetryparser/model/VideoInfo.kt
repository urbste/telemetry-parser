package io.github.telemetryparser.model

/**
 * Video information from the source file.
 *
 * @property frameCount Total number of video frames
 * @property fpsNumerator Frame rate numerator
 * @property fpsDenominator Frame rate denominator (e.g. 1001 for 30000/1001 fps)
 */
data class VideoInfo(
    val frameCount: Int,
    val fpsNumerator: Int,
    val fpsDenominator: Int
) {
    val fps: Float
        get() = if (fpsDenominator > 0) fpsNumerator.toFloat() / fpsDenominator else 0f

    val durationSeconds: Float
        get() = if (fps > 0) frameCount / fps else 0f
}
