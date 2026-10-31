import Foundation
import TelemetryParserFFI

public struct GpsPoint: Codable, Sendable {
    public let latitude: Double
    public let longitude: Double
    public let altitude: Double
    public let speed2d: Double
    public let timestamp: Double
    public let utcTime: Int64
    public let fix: Int32
    public let precision: Int32

    public var speedKmh: Double { speed2d * 3.6 }
    public var speedMph: Double { speed2d * 2.237 }
}

public struct SensorSample: Codable, Sendable {
    public let timestampNs: Int64
    public let x: Float
    public let y: Float
    public let z: Float

    public var magnitude: Double {
        sqrt(Double(x * x + y * y + z * z))
    }
}

public struct OrientationSample: Codable, Sendable {
    public let timestampNs: Int64
    public let w: Double
    public let x: Double
    public let y: Double
    public let z: Double
}

public struct VideoInfo: Codable, Sendable {
    public let frameCount: Int32
    public let fpsNumerator: Int32
    public let fpsDenominator: Int32

    public var fps: Float {
        guard fpsDenominator > 0 else { return 0 }
        return Float(fpsNumerator) / Float(fpsDenominator)
    }

    public var durationSeconds: Float {
        guard fps > 0 else { return 0 }
        return Float(frameCount) / fps
    }
}

public enum TelemetryParserError: Error, LocalizedError {
    case notOpen
    case openFailed(String)
    case nativeCallFailed(String)
    case invalidResponse(String)

    public var errorDescription: String? {
        switch self {
        case .notOpen:
            return "No file is open. Call open(path:) first."
        case .openFailed(let message):
            return message
        case .nativeCallFailed(let message):
            return message
        case .invalidResponse(let message):
            return message
        }
    }
}

private func takeString(_ ptr: UnsafeMutablePointer<CChar>?) -> String? {
    guard let ptr else { return nil }
    defer { telemetry_parser_free_string(ptr) }
    return String(cString: ptr)
}

func decodeArrayJSON<T: Decodable>(_ json: String?, nativeError: String? = nil) throws -> [T] {
    guard let json else {
        if let nativeError { throw TelemetryParserError.nativeCallFailed(nativeError) }
        throw TelemetryParserError.invalidResponse("Native parser returned no JSON array")
    }
    guard let data = json.data(using: .utf8) else {
        throw TelemetryParserError.invalidResponse("Invalid UTF-8 in JSON response")
    }
    do {
        return try JSONDecoder().decode([T].self, from: data)
    } catch {
        throw TelemetryParserError.invalidResponse("Could not decode JSON array: \(error.localizedDescription)")
    }
}

func decodeOptionalJSON<T: Decodable>(_ json: String?, nativeError: String? = nil) throws -> T? {
    guard let json else {
        if let nativeError { throw TelemetryParserError.nativeCallFailed(nativeError) }
        return nil
    }
    guard let data = json.data(using: .utf8) else {
        throw TelemetryParserError.invalidResponse("Invalid UTF-8 in JSON response")
    }
    do {
        return try JSONDecoder().decode(T.self, from: data)
    } catch {
        throw TelemetryParserError.invalidResponse("Could not decode JSON response: \(error.localizedDescription)")
    }
}

/// Parser for telemetry metadata embedded in action camera video files.
///
/// Supports GoPro, DJI, and Insta360 formats. Sensor timestamps are in **nanoseconds**
/// from the start of the clip; GPS follows gpmf_android-style fields.
public final class TelemetryParser: @unchecked Sendable {
    // The native parser handle and LAST_ERROR are process-global native state. Serialize
    // complete FFI + error-read sequences across parser instances.
    private static let ffiLock = NSRecursiveLock()
    private var nativeHandle: UInt64 = 0
    private var isOpen = false

    public init() {}

    deinit {
        close()
    }

    private func synchronized<T>(_ body: () throws -> T) rethrows -> T {
        Self.ffiLock.lock()
        defer { Self.ffiLock.unlock() }
        return try body()
    }

    private func nativeErrorMessage() -> String? {
        takeString(telemetry_parser_last_error())
    }

    private func decodeArray<T: Decodable>(_ ptr: UnsafeMutablePointer<CChar>?) throws -> [T] {
        let json = takeString(ptr)
        return try decodeArrayJSON(json, nativeError: json == nil ? nativeErrorMessage() : nil)
    }

    private func decodeOptional<T: Decodable>(_ ptr: UnsafeMutablePointer<CChar>?) throws -> T? {
        let json = takeString(ptr)
        return try decodeOptionalJSON(json, nativeError: json == nil ? nativeErrorMessage() : nil)
    }

    private func optionalString(_ ptr: UnsafeMutablePointer<CChar>?) throws -> String? {
        guard let value = takeString(ptr) else {
            if let error = nativeErrorMessage() { throw TelemetryParserError.nativeCallFailed(error) }
            return nil
        }
        return value
    }

    /// Open a video file at a filesystem path.
    ///
    /// On iOS, copy Photos or document-picker assets to a temp file before calling this.
    public func open(path: String) throws {
        try synchronized {
            close()
            path.withCString { cPath in
                nativeHandle = telemetry_parser_open(cPath)
            }
            if nativeHandle == 0 {
                let message = nativeErrorMessage() ?? "Failed to open file or unsupported format: \(path)"
                throw TelemetryParserError.openFailed(message)
            }
            isOpen = true
        }
    }

    public func close() {
        synchronized {
            if isOpen && nativeHandle != 0 {
                telemetry_parser_close(nativeHandle)
                nativeHandle = 0
                isOpen = false
            }
        }
    }

    public var opened: Bool { synchronized { isOpen } }

    public func getCameraType() throws -> String {
        try synchronized {
            try checkOpen()
            guard let camera = takeString(telemetry_parser_get_camera_type(nativeHandle)) else {
                throw TelemetryParserError.nativeCallFailed(nativeErrorMessage() ?? "Native parser returned no camera type")
            }
            return camera
        }
    }

    public func getDuration() throws -> Float {
        try synchronized {
            try checkOpen()
            let duration = telemetry_parser_get_duration(nativeHandle)
            if let error = nativeErrorMessage() { throw TelemetryParserError.nativeCallFailed(error) }
            return duration
        }
    }

    public func getVideoInfo() throws -> VideoInfo? {
        try synchronized {
            try checkOpen()
            return try decodeOptional(telemetry_parser_get_video_info_json(nativeHandle))
        }
    }

    /// Recording metadata JSON when present, or nil when the file has no recording metadata.
    public func getRecordingMetadataJson() throws -> String? {
        try synchronized {
            try checkOpen()
            return try optionalString(telemetry_parser_get_recording_metadata_json(nativeHandle))
        }
    }

    /// Camera model when present, or nil when the file does not identify a model.
    public func getCameraModel() throws -> String? {
        try synchronized {
            try checkOpen()
            return try optionalString(telemetry_parser_get_camera_model(nativeHandle))
        }
    }

    /// DVID / FOVL lens JSON when present, or nil.
    public func getLensMetadataJson() throws -> String? {
        try synchronized {
            try checkOpen()
            return try optionalString(telemetry_parser_get_lens_metadata_json(nativeHandle))
        }
    }

    public func getGpsData() throws -> [GpsPoint] {
        try synchronized {
            try checkOpen()
            return try decodeArray(telemetry_parser_get_gps_json(nativeHandle))
        }
    }

    public func getAccelerometerData() throws -> [SensorSample] {
        try synchronized {
            try checkOpen()
            return try decodeArray(telemetry_parser_get_accelerometer_json(nativeHandle))
        }
    }

    public func getGyroscopeData() throws -> [SensorSample] {
        try synchronized {
            try checkOpen()
            return try decodeArray(telemetry_parser_get_gyroscope_json(nativeHandle))
        }
    }

    public func getGravityVectorData() throws -> [SensorSample] {
        try synchronized {
            try checkOpen()
            return try decodeArray(telemetry_parser_get_gravity_json(nativeHandle))
        }
    }

    /// CORI: camera orientation quaternions since capture start.
    public func getCameraOrientationData() throws -> [OrientationSample] {
        try synchronized {
            try checkOpen()
            return try decodeArray(telemetry_parser_get_camera_orientation_json(nativeHandle))
        }
    }

    /// IORI: image orientation relative to camera body.
    public func getImageOrientationData() throws -> [OrientationSample] {
        try synchronized {
            try checkOpen()
            return try decodeArray(telemetry_parser_get_image_orientation_json(nativeHandle))
        }
    }

    /// CORI × IORI product (when both streams exist).
    public func getOrientationCombinedData() throws -> [OrientationSample] {
        try synchronized {
            try checkOpen()
            return try decodeArray(telemetry_parser_get_orientation_combined_json(nativeHandle))
        }
    }

    private func checkOpen() throws {
        if !isOpen {
            throw TelemetryParserError.notOpen
        }
    }
}

public func withTelemetryParser<T>(
    path: String,
    _ block: (TelemetryParser) throws -> T
) throws -> T {
    let parser = TelemetryParser()
    defer { parser.close() }
    try parser.open(path: path)
    return try block(parser)
}
