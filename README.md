# Origami

Origami is a cross-platform archive manager built with **Tauri 2 + Rust + React/TypeScript**, aiming for a lightweight daily-use experience close to Bandizip / 7-Zip.

## Features

- **Browse & extract**: browse the file system and archive directory trees within the app, with support for extract-all/extract-selected, preview, opening with the default program, and integrity verification.
- **Create & edit**: compress files or folders, add or remove entries from existing archives, with configurable compression level, password, split volumes, and system junk-file filtering.
- **Archive formats**:
  - Read/write: `zip` (with AES), `7z` (with AES-256), `tar`, `gz`/`tgz`, `bz2`/`tbz2`, `xz`/`txz`, `zst`/`tzst`, `jar`, `apk`
  - Read-only: `rar`
- **Encoding handling**: automatically detects non-UTF-8 filename encodings, or you can manually choose GBK, Big5, Shift-JIS, and others.
- **Password protection**: plaintext passwords are saved to the macOS Keychain or Windows Credential Manager; the local `passwords.json` only stores an index. Viewing plaintext requires system authentication such as Touch ID or Windows Hello.
- **System integration**: manage file associations; compress or extract directly from the Finder / Explorer right-click menu; quick tasks use a dedicated progress window.
- **UI settings**: supports light/dark mode, themes, fonts, zoom, and window materials.
- **Internationalization foundation**: language can follow the system, or be manually set to Simplified Chinese or English; the welcome page, app shell, task status, and settings page are wired up for i18n, with the rest of the UI being migrated to the same resource structure.

## Tech stack

| Layer | Technology |
| --- | --- |
| Desktop framework | Tauri 2 |
| Backend | Rust (`zip`, `sevenz-rust2`, `unrar`, `tar`, `flate2`, `bzip2`, `liblzma`, `zstd`) |
| Frontend | React 18 + TypeScript + Vite 6 |
| Internationalization | i18next + react-i18next |
| System authentication | macOS LocalAuthentication / Windows Hello |

## Development

Requires Node.js, the Rust toolchain, and the Tauri 2 system dependencies for your current platform.

```bash
npm install
npm run tauri dev
npm run tauri build
```

Before committing changes, run at least:

```bash
npx tsc --noEmit
cd src-tauri && cargo check
```

The current platform can't cover the `#[cfg(...)]` branches of the other platform; for changes touching system integration, you also need to do runtime verification on the target system. Windows requires the MSVC Rust toolchain, the WebView2 Runtime, and Visual Studio Build Tools with the C++ workload.

## Project structure

```text
src/
  App.tsx                 Main window, archive tasks, and global interactions
  main.tsx                Entry points for main window / quick progress window / interactive task window
  i18n/                   Language resolution and translation resources
  components/
    Welcome.tsx           Welcome page and recently opened
    FileExplorer.tsx      File system browsing
    Browser.tsx           Archive content browsing and editing
    dialogs.tsx           Settings, compress, extract, password, and other dialogs
    MiniProgress.tsx      Quick task progress window
    AskDialog.tsx         Quick task window requiring user configuration
  api.ts                  Tauri command wrappers
  settings.ts             Persisted settings

src-tauri/src/
  archive/                Archive detection, listing, extraction, creation, editing, and preview
  cli.rs                  Command-line and deep-link action parsing
  services.rs             macOS Finder Quick Action
  winmenu.rs              Windows classic right-click menu
  macassoc.rs/winassoc.rs File associations
  sysauth.rs              Touch ID / Windows Hello
  passwords.rs            System credential store and local index
  sysicon.rs              System file icons

windows-extension/        Win11 new top-level menu (IExplorerCommand + sparse package)
finder-extension/         macOS Finder extension code
```

## Internationalization development

- Translation resources live in `src/i18n/locales/`, grouped by feature with stable semantic keys — don't use the Chinese source text as the key.
- When adding new user-facing text, update both `zh-CN.ts` and `en-US.ts` together; brand names, file paths, format names, and raw backend error text don't need translation.
- Use `useTranslation()` inside components; non-React code can import the `i18n` instance directly. Text with counts or variables should be generated via interpolation, not by concatenating sentences in JSX.
- `settings.language` stores `system`, `zh-CN`, or `en-US`. `system` resolves the browser's preferred language, and currently unsupported languages fall back to English.
- After completing a batch of migration, check layout, button widths, empty states, and task dialogs in both Chinese and English.

## Platform integration

- **macOS**: the Finder Quick Action can be installed/removed to add compress and extract actions. Since LaunchServices may bring an already-installed version with the same bundle id to the foreground, local UI verification should always be done against the most recently built and installed `.app`.
- **Windows**: the app can install/remove an HKCU-based classic right-click menu; the Win11 new top-level menu requires signing and sparse package registration — see [`windows-extension/README.md`](windows-extension/README.md).
- **Password storage**: legacy plaintext indexes are migrated to the system credential store on read or write; after a successful migration, the local file no longer retains plaintext.
