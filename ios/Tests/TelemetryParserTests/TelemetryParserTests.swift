import XCTest
@testable import TelemetryParser

final class TelemetryParserTests: XCTestCase {
    func testGPSUnitsAndOrientationQuaternionFieldOrder() throws {
        let gps: [GpsPoint] = try decodeArrayJSON(
            #"[{"latitude":1.25,"longitude":-2.5,"altitude":12,"speed2d":10,"timestamp":1234567890,"utcTime":1700000000000,"fix":3,"precision":4}]"#
        )
        XCTAssertEqual(gps.count, 1)
        XCTAssertEqual(gps[0].speedKmh, 36, accuracy: 0.0001)
        XCTAssertEqual(gps[0].speedMph, 22.37, accuracy: 0.001)
        XCTAssertEqual(gps[0].timestamp, 1_234_567_890)
        XCTAssertEqual(gps[0].utcTime, 1_700_000_000_000)

        let orientations: [OrientationSample] = try decodeArrayJSON(
            #"[{"timestampNs":2500000000,"w":0.5,"x":0.1,"y":0.2,"z":0.3}]"#
        )
        XCTAssertEqual(orientations[0].timestampNs, 2_500_000_000)
        XCTAssertEqual(orientations[0].w, 0.5)
        XCTAssertEqual(orientations[0].x, 0.1)
        XCTAssertEqual(orientations[0].y, 0.2)
        XCTAssertEqual(orientations[0].z, 0.3)
    }

    func testFrameRateRetainsRationalNTSCValues() throws {
        let info: VideoInfo = try XCTUnwrap(decodeOptionalJSON(
            #"{"frameCount":300,"fpsNumerator":30000,"fpsDenominator":1001}"#
        ))
        XCTAssertEqual(info.frameCount, 300)
        XCTAssertEqual(info.fpsNumerator, 30_000)
        XCTAssertEqual(info.fpsDenominator, 1_001)
        XCTAssertEqual(Double(info.fps), 30_000.0 / 1_001.0, accuracy: 0.00001)
        XCTAssertEqual(Double(info.durationSeconds), 10.01, accuracy: 0.001)
    }

    func testOptionalMetadataDistinguishesAbsenceFromNativeFailure() throws {
        let absent: GpsPoint? = try decodeOptionalJSON(nil)
        XCTAssertNil(absent)

        XCTAssertThrowsError(try decodeOptionalJSON(nil, nativeError: "Invalid handle") as GpsPoint?)
    }

    func testMalformedAndMissingArrayResponsesThrow() {
        XCTAssertThrowsError(try decodeArrayJSON("not-json") as [GpsPoint])
        XCTAssertThrowsError(try decodeArrayJSON(nil) as [GpsPoint])
        XCTAssertThrowsError(try decodeArrayJSON(nil, nativeError: "Invalid handle") as [GpsPoint])
    }
}
