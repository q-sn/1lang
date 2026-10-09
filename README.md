# 1lang

Fast, high-quality open-source translator for Windows — a lightweight alternative to the DeepL desktop app.
Built with Rust + Tauri 2, Vue 3, Reka UI and Tailwind CSS v4.

## Features

- **Translator window** in the style of DeepL: translate as you type, auto-detected direction, language swap, tone, provider choice, history.
- **Selected text** in any app:
  - floating button after selecting text with the mouse (next to the selection or the cursor);
  - hotkey (`Ctrl+Alt+T` by default);
  - **Ctrl+C+C** — detected from clipboard changes (no keyboard hook) or with a keyboard hook (configurable).
- **Replace selection with its translation** (`Ctrl+Alt+R`) — write in your language, get it replaced in place.
- **Screen area → OCR → translation** (`Ctrl+Alt+O`) with the built-in Windows OCR.
- **Popup** with streaming output, pin mode, language picker, copy / replace / listen.
- **Dictionary card** for single words (meanings, IPA, examples) with AI providers.
- **Context**: surrounding paragraphs of the selection are sent to the AI to resolve ambiguity (optional).
- **Providers**, combined into a fallback chain:
  - AI: Groq (default), OpenAI, OpenRouter, Gemini, Anthropic, Ollama, LM Studio, any OpenAI-compatible API;
  - machine translation: DeepL, Google Cloud, Microsoft Translator, Google (free, no key).
- **Usage & cost** per provider (today / month / all time), translation cache, history with favorites.
- Excluded apps, Mica / Acrylic glass on Windows 11, light / dark theme, UI in English, Russian and Spanish.

API keys are stored in Windows Credential Manager, never in plain files.

## Development

Requirements: Rust (stable), Node.js 20+, pnpm, WebView2 (preinstalled on Windows 10/11).

```bash
pnpm install
pnpm tauri dev      # run with hot reload
pnpm tauri build    # NSIS installer in target/release/bundle
```

Other commands:

```bash
pnpm bindings                      # regenerate src/bindings.ts from Rust commands
cargo test --workspace             # Rust tests
pnpm typecheck                     # vue-tsc
cargo run -p onelang-core --example translate -- "Hello" groq      # try a provider (GROQ_API_KEY)
cargo run -p onelang-platform --example ocr -- image.png            # try Windows OCR
```

## Structure

```
crates/
  onelang-core/       translation engine, no Tauri / OS code
    providers/        OpenAI-compatible (all LLMs), DeepL, Google, Microsoft
    engine.rs         mode, direction (auto-swap), fallback chain, cost
    prompt.rs         prompts, prompt-injection protection, streamed header parsing
    detect.rs         offline language detection
  onelang-platform/   Windows integration
    selection.rs      UI Automation selection (+ bounds, context) with Ctrl+C fallback
    clipboard.rs      snapshot / restore, private writes
    hooks.rs          low-level keyboard / mouse hooks
    clipboard_watch   clipboard change listener (Ctrl+C+C)
    ocr.rs            Windows.Media.Ocr, paragraph joining, multi-language auto mode
src-tauri/            Tauri app: commands, windows, triggers, settings, SQLite, tray
src/                  Vue frontend: windows/{main,popup,icon,region}, ui/, lib/, i18n/
```

## License

MIT
