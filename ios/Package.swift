// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "TelemetryParser",
    platforms: [
        .iOS(.v13),
        .macOS(.v11),
    ],
    products: [
        .library(
            name: "TelemetryParser",
            targets: ["TelemetryParser"]
        ),
    ],
    targets: [
        .binaryTarget(
            name: "TelemetryParserFFI",
            path: "build/TelemetryParser.xcframework"
        ),
        .target(
            name: "TelemetryParser",
            dependencies: ["TelemetryParserFFI"],
            path: "Sources/TelemetryParser"
        ),
    ]
)
