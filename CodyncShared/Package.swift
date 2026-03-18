// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "CodyncShared",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "CodyncShared", targets: ["CodyncShared"]),
    ],
    targets: [
        .target(name: "CodyncShared", path: "Sources"),
        .testTarget(name: "CodyncSharedTests", dependencies: ["CodyncShared"], path: "Tests"),
    ]
)
