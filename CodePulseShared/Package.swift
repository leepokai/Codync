// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "CodePulseShared",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "CodePulseShared", targets: ["CodePulseShared"]),
    ],
    targets: [
        .target(name: "CodePulseShared", path: "Sources"),
        .testTarget(name: "CodePulseSharedTests", dependencies: ["CodePulseShared"], path: "Tests"),
    ]
)
