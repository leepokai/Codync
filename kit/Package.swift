// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "CodyncKit",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "CodyncKit", targets: ["CodyncKit"]),
        .library(name: "CodyncUI", targets: ["CodyncUI"]),
    ],
    targets: [
        .target(name: "CodyncKit"),
        .target(name: "CodyncUI", dependencies: ["CodyncKit"], resources: [.process("Resources")]),
        .testTarget(name: "CodyncKitTests", dependencies: ["CodyncKit"], resources: [.copy("Fixtures")]),
    ]
)
