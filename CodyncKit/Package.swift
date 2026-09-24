// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "CodyncKit",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "CodyncKit", targets: ["CodyncKit"]),
    ],
    targets: [
        .target(name: "CodyncKit"),
        .testTarget(name: "CodyncKitTests", dependencies: ["CodyncKit"], resources: [.copy("Fixtures")]),
    ]
)
