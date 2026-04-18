package io.github.telemetryparser

import io.github.telemetryparser.model.GpsPoint
import io.github.telemetryparser.model.OrientationSample
import io.github.telemetryparser.model.SensorSample
import io.github.telemetryparser.model.VideoInfo
import java.io.Closeable

/**
 * Parser for telemetry metadata embedded in action camera video files.
 *
 * Supports GoPro, DJI, and Insta360 formats. Sensor timestamps are in **nanoseconds**
 * from the start of the clip; GPS follows gpmf_android-style fields.
 */
class TelemetryParser : Closeable {

    companion object {
        init {
            System.loadLibrary("telemetry_parser")
        }
    }

    private var nativeHandle: Long = 0
    private var isOpen: Boolean = false

    @Throws(java.io.IOException::class)
    fun open(filePath: String) {
        if (isOpen) {
            close()
        }

        nativeHandle = nativeOpen(filePath)
        if (nativeHandle == 0L) {
            throw java.io.IOException("Failed to open file or unsupported format: $filePath")
        }
        isOpen = true
    }

    override fun close() {
        if (isOpen && nativeHandle != 0L) {
            nativeClose(nativeHandle)
            nativeHandle = 0
            isOpen = false
        }
    }

    fun isOpen(): Boolean = isOpen

    fun getCameraType(): String {
        checkOpen()
        return nativeGetCameraType(nativeHandle) ?: "Unknown"
    }

    fun getDuration(): Float {
        checkOpen()
        return nativeGetDuration(nativeHandle)
    }

    fun getVideoInfo(): VideoInfo? {
        checkOpen()
        return nativeGetVideoInfo(nativeHandle)
    }

    /**
     * DVID / FOVL lens JSON when present (same structure as extract-metadata `lens` object), or null.
     */
    fun getLensMetadataJson(): String? {
        checkOpen()
        return nativeGetLensMetadataJson(nativeHandle)
    }

    fun getGpsData(): List<GpsPoint> {
        checkOpen()
        return nativeGetGpsData(nativeHandle)?.toList() ?: emptyList()
    }

    fun getAccelerometerData(): List<SensorSample> {
        checkOpen()
        return nativeGetAccelerometerData(nativeHandle)?.toList() ?: emptyList()
    }

    fun getGyroscopeData(): List<SensorSample> {
        checkOpen()
        return nativeGetGyroscopeData(nativeHandle)?.toList() ?: emptyList()
    }

    fun getGravityVectorData(): List<SensorSample> {
        checkOpen()
        return nativeGetGravityVectorData(nativeHandle)?.toList() ?: emptyList()
    }

    /** CORI: camera orientation quaternions since capture start. */
    fun getCameraOrientationData(): List<OrientationSample> {
        checkOpen()
        return nativeGetCameraOrientationData(nativeHandle)?.toList() ?: emptyList()
    }

    /** IORI: image orientation relative to camera body. */
    fun getImageOrientationData(): List<OrientationSample> {
        checkOpen()
        return nativeGetImageOrientationData(nativeHandle)?.toList() ?: emptyList()
    }

    /** CORI × IORI product (when both streams exist). */
    fun getOrientationCombinedData(): List<OrientationSample> {
        checkOpen()
        return nativeGetOrientationCombinedData(nativeHandle)?.toList() ?: emptyList()
    }

    private fun checkOpen() {
        if (!isOpen) {
            throw IllegalStateException("No file is open. Call open() first.")
        }
    }

    private external fun nativeOpen(filePath: String): Long
    private external fun nativeClose(handle: Long)
    private external fun nativeGetCameraType(handle: Long): String?
    private external fun nativeGetDuration(handle: Long): Float
    private external fun nativeGetVideoInfo(handle: Long): VideoInfo?
    private external fun nativeGetLensMetadataJson(handle: Long): String?
    private external fun nativeGetGpsData(handle: Long): Array<GpsPoint>?
    private external fun nativeGetAccelerometerData(handle: Long): Array<SensorSample>?
    private external fun nativeGetGyroscopeData(handle: Long): Array<SensorSample>?
    private external fun nativeGetGravityVectorData(handle: Long): Array<SensorSample>?
    private external fun nativeGetCameraOrientationData(handle: Long): Array<OrientationSample>?
    private external fun nativeGetImageOrientationData(handle: Long): Array<OrientationSample>?
    private external fun nativeGetOrientationCombinedData(handle: Long): Array<OrientationSample>?
}

inline fun <T> withTelemetryParser(filePath: String, block: (TelemetryParser) -> T): T {
    return TelemetryParser().use { parser ->
        parser.open(filePath)
        block(parser)
    }
}
