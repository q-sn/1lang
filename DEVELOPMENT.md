# Development

1lang is a Tauri 2 app: Rust backend, Vue 3 + Reka UI + Tailwind CSS v4 frontend.

## Setup

Requirements: Rust (stable), Node.js 20+, pnpm 10, WebView2 (preinstalled on Windows 10/11).

```bash
pnpm install
pnpm tauri dev      # run with hot reload
pnpm tauri build    # NSIS installer in target/release/bundle
```

Other commands:

```bash
pnpm bindings                      # regenerate src/bindings.ts from Rust commands
pnpm typecheck                     # vue-tsc
cargo test -p onelang-core -p onelang-platform
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p onelang-core --example translate -- "Hello" groq    # try a provider (GROQ_API_KEY)
cargo run -p onelang-platform --example ocr -- image.png          # try Windows OCR
```

## Project layout

```
crates/
  onelang-core/       translation engine, no Tauri / OS code
    providers/        OpenAI-compatible (all LLMs), DeepL, Google, Microsoft
    engine.rs         mode, direction (auto-swap), fallback chain, cost
    prompt.rs         prompts, prompt-injection protection, streamed header parsing
    detect.rs         offline language detection
  onelang-platform/   Windows integration
    selection.rs      UI Automation selection (bounds, context) with Ctrl+C fallback
    clipboard.rs      snapshot / restore, private writes
    hooks.rs          low-level keyboard / mouse hooks
    clipboard_watch   clipboard change listener (Ctrl+C+C)
    ocr.rs            Windows.Media.Ocr, paragraph joining, multi-language auto mode
src-tauri/            Tauri app: commands, windows, triggers, settings, SQLite, tray, updater
src/                  Vue frontend: windows/{main,popup,icon,region}, ui/, lib/, i18n/
```

Settings (`settings.json`) and history/cache (`1lang.db`) live in `%APPDATA%\dev.onelang.app`,
API keys in Windows Credential Manager.

## Releases

```bash
pnpm release 0.2.0
```

Bumps the version in `package.json`, `tauri.conf.json` and `Cargo.toml`, commits, tags and pushes.
The tag triggers `.github/workflows/release.yml`, which builds the installer, signs the updater
artifacts with the `TAURI_SIGNING_PRIVATE_KEY` secret and publishes a GitHub Release with
`latest.json`. Installed apps download the update in the background and install it on exit.
