// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "TelemetrySample",
    platforms: [
        .iOS(.v13),
        .macOS(.v11),
    ],
    dependencies: [
        .package(path: "../ios"),
    ],
    products: [
        .executable(name: "TelemetrySampleCLI", targets: ["TelemetrySampleCLI"]),
    ],
    targets: [
        .executableTarget(
            name: "TelemetrySampleCLI",
            dependencies: [
                .product(name: "TelemetryParser", package: "TelemetryParser"),
            ],
            path: "Sources/CLI"
        ),
    ]
)
