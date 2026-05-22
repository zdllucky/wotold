# Tauri sidecar binaries

External binaries bundled with the Wotold app. Per `tauri.conf.json::bundle.externalBin`,
each entry resolves to `<name>-<target-triple>` (e.g. `wotold-audio-aarch64-apple-darwin`).

## Inventory

- `wotold-audio` — Swift Core Audio process tap sidecar. Built by
  `apps/desktop/sidecars/macos-audio/scripts/build.sh`. Required.

- `wotold-llama` — llama.cpp `llama-cli` binary (M12.3). **Required for Local
  engine recap.** Placeholder stub is committed so `cargo build` succeeds in
  CI without the heavyweight binary; replace with a real llama.cpp build
  before release.

- `wotold-whisper` — whisper.cpp `whisper-cli` binary (M12.1). **Required for
  Local engine STT.** sherpa-onnx Whisper requires encoder/decoder ONNX pair,
  but our model catalog targets ggerganov .bin format (Whisper SHA256
  reachable through `scripts/refresh-model-catalog.sh`). Sidecar pattern keeps
  catalog stable and matches the llama integration. Placeholder stub is
  committed; replace before release.

## Producing `wotold-llama` (release prep)

1. Clone llama.cpp:
   ```bash
   git clone https://github.com/ggml-org/llama.cpp.git
   cd llama.cpp
   ```

2. Build for Apple Silicon (Metal-accelerated):
   ```bash
   cmake -B build -DLLAMA_METAL=ON -DCMAKE_BUILD_TYPE=Release
   cmake --build build --config Release -j --target llama-cli
   ```

3. Copy + rename binary into this directory:
   ```bash
   cp build/bin/llama-cli \
      /path/to/wotold/apps/desktop/src-tauri/binaries/wotold-llama-aarch64-apple-darwin
   ```

4. For x86_64 builds (Intel Mac CI) — repeat with `-DCMAKE_OSX_ARCHITECTURES=x86_64`
   and rename to `wotold-llama-x86_64-apple-darwin`.

## Producing `wotold-whisper` (release prep)

1. Clone whisper.cpp:
   ```bash
   git clone https://github.com/ggml-org/whisper.cpp.git
   cd whisper.cpp
   ```

2. Build for Apple Silicon (Core ML / Metal):
   ```bash
   cmake -B build -DCMAKE_BUILD_TYPE=Release -DWHISPER_METAL=ON
   cmake --build build --config Release -j --target whisper-cli
   ```

3. Copy + rename:
   ```bash
   cp build/bin/whisper-cli \
      /path/to/wotold/apps/desktop/src-tauri/binaries/wotold-whisper-aarch64-apple-darwin
   ```

4. For x86_64 — `-DCMAKE_OSX_ARCHITECTURES=x86_64`, rename to
   `wotold-whisper-x86_64-apple-darwin`.

## CLI contracts

### wotold-llama (llama.cpp)

```
-m <model.gguf>
--temp <float>
--ctx-size <int>
--n-predict <int>
--threads <int>
--no-conversation
--no-display-prompt
--simple-io
--log-disable
-f <prompt-file>
```

Stable across llama.cpp ≥ commit 2025-01. Output: JSON-only text on stdout
(per `LOCAL_LLM_SYSTEM_PROMPT`). Anything else → Provider error.

### wotold-whisper (whisper.cpp)

```
-m <model.bin>            # ggerganov .bin format (ggml-*.bin)
-f <audio.wav>            # 16 kHz mono WAV
--output-json-full        # emit <stem>.json with segments + word timestamps
-of <stem>                # output file stem (extension added by whisper-cli)
-l <lang>                 # BCP47 short code (ru / en / auto)
--threads <int>
--no-prints
--print-progress false
```

Process emits `<stem>.json` to disk; Rust reads and parses after Terminated.

