# Wotold Audio Sidecar

Swift-бинарь, который записывает микрофон в WAV-файл по командам через stdin. Запускается Tauri-приложением как sidecar (см. `apps/desktop/src-tauri/tauri.conf.json` → `bundle.externalBin`).

См. M1.2 паспорта. System audio (ScreenCaptureKit) подключается в #15c.

## Сборка

```bash
./scripts/build-audio-sidecar.sh
```

Скрипт делает `swift build -c release` и копирует бинарь в `apps/desktop/src-tauri/binaries/wotold-audio-<target-triple>` (Tauri convention).

## Протокол

Все сообщения — одна JSON-строка на строку (`\n`-разделитель).

### stdin commands

| Команда | Аргументы | Что делает |
|---|---|---|
| `{"cmd":"ping"}` | — | проверка живости |
| `{"cmd":"start","mic_path":"/abs/mic.wav"}` | абсолютный путь | стартует запись микрофона в `mic_path` (16-bit PCM mono 16 kHz) |
| `{"cmd":"stop"}` | — | стопает запись, дозаписывает RIFF-заголовок |

### stdout events

| Событие | Поля | Когда |
|---|---|---|
| `{"event":"pong"}` | — | в ответ на ping |
| `{"event":"started"}` | — | подтверждение start |
| `{"event":"stopped","duration_sec":N,"mic_bytes":N}` | — | подтверждение stop |
| `{"event":"error","message":"..."}` | — | любая ошибка (продолжаем работать, не падаем) |

## Permissions

`Info.plist` встроен в бинарь через linker flag `-sectcreate __TEXT __info_plist`. Содержит `NSMicrophoneUsageDescription` — macOS покажет диалог запроса доступа к микрофону при первом `start`.

Если пользователь отказал — `start` вернёт `error` события и продолжит работать. Гранул проверки доступа на старте (без записи) MVP не имеет — это #16.

## Локальный smoke-test

```bash
./scripts/build-audio-sidecar.sh
echo '{"cmd":"ping"}' | ./apps/desktop/src-tauri/binaries/wotold-audio-aarch64-apple-darwin
# → {"event":"pong"}
```
