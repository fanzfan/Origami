# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Origami is a cross-platform (macOS / Windows) archive manager: Tauri 2 + Rust backend in `src-tauri/`, React 18 + TypeScript + Vite frontend in `src/`.

The rules shared by every coding agent (verification, cross-platform checklists, i18n, code conventions) live in `AGENTS.md` and are imported below. Add rules that apply to all agents there, not here; this file only adds commands and the architecture that takes several files to piece together.

@AGENTS.md

## Commands

```bash
npm install
npm run tauri dev                 # Vite on :1420 (strictPort) + the Rust app
npx tsc --noEmit                  # frontend typecheck; no linter/formatter is configured
cd src-tauri && cargo check
```

Tests are Rust-only (there are no frontend tests):

```bash
cd src-tauri && cargo test                                           # unit + integration tests
cd src-tauri && cargo test --lib                                     # unit tests only: archive/{mod,list,volume}.rs + lib.rs
cd src-tauri && cargo test --test engine                             # create/list/extract/edit round-trips per format
cd src-tauri && cargo test --lib content_wins_over_wrong_extension   # one test, by name filter
```

- `tests/engine.rs` calls the `archive::*` functions directly with `tauri::test::mock_app()`. When you change `CreateOptions` / `ExtractOptions` / `ListOptions` or a function signature in `archive/`, update the helpers at the top of that file too; `cargo check` doesn't build tests and won't catch the drift.
- `tests/services.rs` (Finder Quick Actions; it shells out to `plutil`) is `#![cfg(target_os = "macos")]` and runs 0 tests elsewhere.
- On Windows, integration-test binaries need the Common Controls v6 manifest that tauri-build only embeds into bin targets. `build.rs` adds it to test targets; without it, the test exe dies at startup with `STATUS_ENTRYPOINT_NOT_FOUND` (`TaskDialogIndirect`).

Packaging and platform extras:

- `npm run tauri build`: bundles per `src-tauri/tauri.conf.json` (`app`, `dmg`, `nsis`).
- `npm run build:mac`: `.app` plus the embedded FinderSync extension (`finder-extension/build.sh`). Run `scripts/setup_local_signing.sh` once first; macOS won't load an ad-hoc-signed Finder extension, and the `.dmg` from `tauri build` doesn't contain it.
- `pwsh -File build_windows_msix.ps1 [-SkipAppBuild] [-Register]`: Origami.exe + NSIS + the Win11 context-menu DLL + a self-signed sparse MSIX. `-Register` needs Developer Mode and admin.
- `cd src-tauri && cargo run --example install_services [-- uninstall]`: install or remove the Finder Quick Actions by hand (macOS).
- Release: pushing a `v*` tag runs `.github/workflows/release.yml` (macOS universal dmg + Windows NSIS into a draft Release). The tag must match `version` in `tauri.conf.json`; the versions in `package.json` and `Cargo.toml` are not used. A manual `workflow_dispatch` only builds and uploads the bundles as artifacts.

A binary built with plain `cargo build`/`cargo run` (not through the tauri CLI) loads its frontend from the Vite dev server at `localhost:1420`. If that server isn't running, no window runs any JS, which looks like a WebView bug. Start `npm run dev` first or use `npm run tauri dev`.

## Architecture

### One bundle, three windows

`src/main.tsx` picks the root component by query string: `?mini=1` → `MiniProgress` (quick-task progress), `?ask=1` → `AskDialog` (compress/extract settings in a standalone window), otherwise `App`. `main` is declared in `tauri.conf.json` and created hidden; `mini` and `ask` are created at runtime by `spawn_mini_window` / `spawn_ask_window` in `lib.rs`. `main.tsx` applies settings itself because `mini`/`ask` never mount `App`. It also sets the `data-platform` / `data-window` / `data-effects` attributes on `<html>` that `styles.css` keys off.

`src-tauri/capabilities/default.json` lists the window labels and the core/plugin permissions. A new window label, or a new frontend call to a window or plugin API, needs an entry there.

### OS launch actions → `PendingAction` → window

Every external entry point turns into `PendingAction { Open | Create{format} | Extract{mode} }` and goes through `dispatch_actions` in `lib.rs`:

- **macOS**: `RunEvent::Opened` delivers file URLs and `origami://create|extract?format=…&mode=…&p=<base64url path>` deep links. The Finder Quick Actions (`services.rs`) and the FinderSync extension produce these.
- **Windows/Linux**: `cli.rs` parses argv: bare paths mean open, `--compress=<zip|7z|…|ask>`, `--extract=<smart|here|folder|ask>`, or a deep-link string. It runs at startup (`setup`) and on single-instance forwarding. The classic menu (`winmenu.rs`) and the Win11 `IExplorerCommand` DLL (`windows-extension/`) only launch `Origami.exe` with these flags.

Routing: actions that need input (`format == "ask"` or `mode == "ask"`) go to the `PendingAsk` queue and are handled by the `ask` window (`take_ask_actions` → `finish_ask_window`). Everything else goes to `Pending`. If every action is quick and `main` is hidden, `QuickLaunch` is set and the visible `mini` window runs the job (`take_pending_actions` → create/extract → `end_quick_job`). `end_quick_job` exits the app on success if `main` was never shown, and shows `main` on failure. Otherwise `main` is shown and `App.tsx` drains the queue on the `deep-link-available` event.

Never drive work from the hidden `main` window. On Windows, a hidden WebView2 may not run JS, and `App`'s drain returns early while the window is invisible so that it doesn't race `mini` for the destructive `take_pending_actions`. For the same reason, runtime windows are built visible with a `background_color` (to avoid a white flash) rather than with `visible: false`.

### Jobs, progress, errors

- Long operations (extract, create, add/remove, test) take a frontend-generated `jobId` (`newJobId()` in `api.ts`). `Jobs` maps it to a cancel `AtomicBool` (`cancel_job`), and the work runs in `spawn_blocking` with `archive::extract::Ctx { app, cancel }`.
- Progress uses a single global `job-progress` event (`{jobId, current, total, file, done}`), emitted with throttling by the `Tracker` in `extract.rs` and consumed through `api.onProgress`. Listeners must filter by `jobId`.
- Errors reach the frontend as strings. `err_str` reduces any error chain containing `PASSWORD_REQUIRED` or `CANCELLED` to exactly that token, and the frontend compares against these tokens to show the password prompt or the "cancelled" toast. Backend code that hits either condition must raise the same token (`anyhow::bail!("CANCELLED")`).
- Passwords: list/extract take an explicit `password` plus `passwords::LazyPasswords`, which reads the credential store only after the explicit password fails, so opening an archive doesn't trigger a Keychain prompt. A successful extract with a typed password saves it (`passwords::add` + `mark_used`).

### Archive engine (`src-tauri/src/archive/`)

- `detect_format` (`mod.rs`) trusts content over the extension. It sniffs magic bytes. For single-stream `.gz/.bz2/.xz/.zst` it decompresses the first 512 bytes to decide between tar and a single file. It also scans for 7z/RAR signatures behind SFX stubs, and treats a file as zip only if its central directory actually opens. The frontend checks the extension first (no IO) and calls `probe_archive` only when the name is inconclusive.
- Split volumes (`volume.rs`): the format comes from the logical name (`foo.7z.001` → `foo.7z`). `VolumeKind` is `numeric` (`.001`), `rarPart`, `rarOld`, or `zipSpanned`, and `ArchiveInfo.volumes` lists any missing parts.
- Every extracted path goes through `sanitize_rel_path`, which strips absolute prefixes and `..`. When writing entry names, join path `.components()` with `/`; `to_string_lossy()` on a `PathBuf` produces backslash entry names on Windows.
- Non-UTF-8 entry names are decoded by `encoding.rs`: `chardetng` when `encoding == "auto"`, otherwise the explicit GBK / Big5 / Shift-JIS choice.
- Serialized structs use `#[serde(rename_all = "camelCase")]`, and `src/api.ts` mirrors them by hand. Change both sides together.

### Platform modules

Platform modules are `#[cfg]`-gated at their `mod` declarations in `lib.rs`: `services`, `macassoc`, `sysicon` are macOS-only; `winassoc`, `winmat`, `winmenu` are Windows-only; `cli` is non-macOS. On the other OS they are not compiled or type-checked at all. On Windows, the main window has its decorations removed in `setup` and the frontend draws the title bar. Adjustable acrylic opacity uses the undocumented `SetWindowCompositionAttribute` (`winmat.rs`) because Tauri's `setEffects` can't change opacity. Settings persist in `localStorage["settings"]` (`src/settings.ts`).
