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
    case invalidResponse(String)

    public var errorDescription: String? {
        switch self {
        case .notOpen:
            return "No file is open. Call open(path:) first."
        case .openFailed(let message):
            return message
        case .invalidResponse(let message):
            return message
        }
    }
}

private enum TelemetryJSON {
    static let decoder = JSONDecoder()
}

private func takeString(_ ptr: UnsafeMutablePointer<CChar>?) -> String? {
    guard let ptr else { return nil }
    defer { telemetry_parser_free_string(ptr) }
    return String(cString: ptr)
}

private func decodeArray<T: Decodable>(_ ptr: UnsafeMutablePointer<CChar>?) throws -> [T] {
    guard let json = takeString(ptr) else { return [] }
    guard let data = json.data(using: .utf8) else {
        throw TelemetryParserError.invalidResponse("Invalid UTF-8 in JSON response")
    }
    return try TelemetryJSON.decoder.decode([T].self, from: data)
}

private func decodeOptional<T: Decodable>(_ ptr: UnsafeMutablePointer<CChar>?) throws -> T? {
    guard let json = takeString(ptr) else { return nil }
    guard let data = json.data(using: .utf8) else {
        throw TelemetryParserError.invalidResponse("Invalid UTF-8 in JSON response")
    }
    return try TelemetryJSON.decoder.decode(T.self, from: data)
}

/// Parser for telemetry metadata embedded in action camera video files.
///
/// Supports GoPro, DJI, and Insta360 formats. Sensor timestamps are in **nanoseconds**
/// from the start of the clip; GPS follows gpmf_android-style fields.
public final class TelemetryParser: @unchecked Sendable {
    private var nativeHandle: UInt64 = 0
    private var isOpen = false

    public init() {}

    deinit {
        close()
    }

    /// Open a video file at a filesystem path.
    ///
    /// On iOS, copy Photos or document-picker assets to a temp file before calling this.
    public func open(path: String) throws {
        close()
        path.withCString { cPath in
            nativeHandle = telemetry_parser_open(cPath)
        }
        if nativeHandle == 0 {
            let message = takeString(telemetry_parser_last_error()) ?? "Failed to open file or unsupported format: \(path)"
            throw TelemetryParserError.openFailed(message)
        }
        isOpen = true
    }

    public func close() {
        if isOpen && nativeHandle != 0 {
            telemetry_parser_close(nativeHandle)
            nativeHandle = 0
            isOpen = false
        }
    }

    public var opened: Bool { isOpen }

    public func getCameraType() throws -> String {
        try checkOpen()
        return takeString(telemetry_parser_get_camera_type(nativeHandle)) ?? "Unknown"
    }

    public func getDuration() throws -> Float {
        try checkOpen()
        return telemetry_parser_get_duration(nativeHandle)
    }

    public func getVideoInfo() throws -> VideoInfo? {
        try checkOpen()
        return try decodeOptional(telemetry_parser_get_video_info_json(nativeHandle))
    }

    /// DVID / FOVL lens JSON when present, or nil.
    public func getLensMetadataJson() throws -> String? {
        try checkOpen()
        return takeString(telemetry_parser_get_lens_metadata_json(nativeHandle))
    }

    public func getGpsData() throws -> [GpsPoint] {
        try checkOpen()
        return try decodeArray(telemetry_parser_get_gps_json(nativeHandle))
    }

    public func getAccelerometerData() throws -> [SensorSample] {
        try checkOpen()
        return try decodeArray(telemetry_parser_get_accelerometer_json(nativeHandle))
    }

    public func getGyroscopeData() throws -> [SensorSample] {
        try checkOpen()
        return try decodeArray(telemetry_parser_get_gyroscope_json(nativeHandle))
    }

    public func getGravityVectorData() throws -> [SensorSample] {
        try checkOpen()
        return try decodeArray(telemetry_parser_get_gravity_json(nativeHandle))
    }

    /// CORI: camera orientation quaternions since capture start.
    public func getCameraOrientationData() throws -> [OrientationSample] {
        try checkOpen()
        return try decodeArray(telemetry_parser_get_camera_orientation_json(nativeHandle))
    }

    /// IORI: image orientation relative to camera body.
    public func getImageOrientationData() throws -> [OrientationSample] {
        try checkOpen()
        return try decodeArray(telemetry_parser_get_image_orientation_json(nativeHandle))
    }

    /// CORI × IORI product (when both streams exist).
    public func getOrientationCombinedData() throws -> [OrientationSample] {
        try checkOpen()
        return try decodeArray(telemetry_parser_get_orientation_combined_json(nativeHandle))
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
