// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "shellder",
    defaultLocalization: "en",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "shellder",
            path: "Sources/shellder",
            exclude: ["Resources"]
        )
    ]
)
