// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "ScreenLink",
    platforms: [.macOS(.v14)],
    targets: [
        .target(name: "ScreenLink", path: "Sources"),
        .testTarget(name: "ScreenLinkTests", dependencies: ["ScreenLink"], path: "Tests"),
    ]
)
