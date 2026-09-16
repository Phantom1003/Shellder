// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "shellder",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "shellder",
            path: "Sources/shellder"
        )
    ]
)
