// swift-tools-version: 5.9
import PackageDescription

// Wotold audio sidecar: записывает микрофон в WAV-файл.
// Управляется через stdin/stdout (JSON line protocol).
// См. apps/desktop/sidecars/macos-audio/README.md.
//
// [TD-06] Три таргета вместо одного executable:
//   WotoldAudioCore — детерминированное ядро протокола (разбор команд,
//     кодирование событий, роутер на протоколах, WAVWriter). Тестируется
//     без Core Audio и без разрешений macOS.
//   WotoldAudio     — тонкий executable: конкретные рекордеры + main().
//   WotoldAudioCoreTests — Swift Testing.
//
// Имя executable-продукта менять НЕЛЬЗЯ: scripts/build-audio-sidecar.sh
// копирует .build/release/WotoldAudio.
//
// Info.plist встроен через linker flag -sectcreate __TEXT __info_plist —
// чтобы macOS показала диалог запроса доступа к микрофону
// (NSMicrophoneUsageDescription) когда AVAudioEngine впервые потребует записи.

let package = Package(
    name: "WotoldAudio",
    platforms: [.macOS(.v14)],
    targets: [
        .target(
            name: "WotoldAudioCore",
            path: "Sources/WotoldAudioCore"
        ),
        .executableTarget(
            name: "WotoldAudio",
            dependencies: ["WotoldAudioCore"],
            path: "Sources/WotoldAudio",
            exclude: ["Info.plist"],
            linkerSettings: [
                .unsafeFlags([
                    "-Xlinker", "-sectcreate",
                    "-Xlinker", "__TEXT",
                    "-Xlinker", "__info_plist",
                    "-Xlinker", "Sources/WotoldAudio/Info.plist",
                ])
            ]
        ),
        .testTarget(
            name: "WotoldAudioCoreTests",
            dependencies: ["WotoldAudioCore"],
            path: "Tests/WotoldAudioCoreTests"
        ),
    ]
)
