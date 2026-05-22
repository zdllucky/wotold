// swift-tools-version: 5.9
import PackageDescription

// Wotold audio sidecar: записывает микрофон в WAV-файл.
// Управляется через stdin/stdout (JSON line protocol).
// См. apps/desktop/sidecars/macos-audio/README.md.
//
// Info.plist встроен через linker flag -sectcreate __TEXT __info_plist —
// чтобы macOS показала диалог запроса доступа к микрофону
// (NSMicrophoneUsageDescription) когда AVAudioEngine впервые потребует записи.

let package = Package(
    name: "WotoldAudio",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "WotoldAudio",
            path: "Sources/WotoldAudio",
            exclude: ["Info.plist"],
            linkerSettings: [
                .linkedFramework("ApplicationServices"),
                .unsafeFlags([
                    "-Xlinker", "-sectcreate",
                    "-Xlinker", "__TEXT",
                    "-Xlinker", "__info_plist",
                    "-Xlinker", "Sources/WotoldAudio/Info.plist",
                ])
            ]
        )
    ]
)
