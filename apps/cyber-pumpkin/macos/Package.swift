// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "CyberPumpkinMac",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "CyberPumpkinMac",
            path: "Sources/CyberPumpkinMac"
        )
    ]
)
