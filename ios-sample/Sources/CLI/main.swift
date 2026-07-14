import Foundation
import TelemetryParser

@main
enum TelemetrySampleCLI {
    static func main() {
        guard CommandLine.arguments.count > 1 else {
            fputs("Usage: TelemetrySampleCLI <video-path>\n", stderr)
            exit(1)
        }

        let path = CommandLine.arguments[1]
        do {
            try withTelemetryParser(path: path) { parser in
                let camera = try parser.getCameraType()
                let duration = try parser.getDuration()
                let gps = try parser.getGpsData()
                let accel = try parser.getAccelerometerData()
                let gyro = try parser.getGyroscopeData()
                let grav = try parser.getGravityVectorData()
                let camOri = try parser.getCameraOrientationData()
                let imgOri = try parser.getImageOrientationData()
                let videoInfo = try parser.getVideoInfo()

                print("Camera: \(camera)")
                print(String(format: "Duration: %.2fs", duration))
                if let info = videoInfo {
                    print("Video: \(info.frameCount) frames @ \(info.fps) fps")
                }
                print("GPS: \(gps.count)  Accel: \(accel.count)  Gyro: \(gyro.count)  Gravity: \(grav.count)")
                print("CORI: \(camOri.count)  IORI: \(imgOri.count)")
                if let lens = try parser.getLensMetadataJson() {
                    print("Lens JSON: \(lens.prefix(120))...")
                }
            }
        } catch {
            fputs("Error: \(error.localizedDescription)\n", stderr)
            exit(1)
        }
    }
}
