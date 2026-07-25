// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "TrackerMac",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "TrackerMac", targets: ["TrackerMac"])
    ],
    targets: [
        .executableTarget(
            name: "TrackerMac",
            path: "Sources/TrackerMac"
        )
    ]
)
