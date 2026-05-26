// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "CloudDriveMount",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "CloudDriveMount", targets: ["CloudDriveMount"])
    ],
    targets: [
        .executableTarget(name: "CloudDriveMount")
    ]
)
