# AGENTS.md

For Codex instances working in this repository. Origami is a cross-platform archive manager built with Tauri 2 + Rust + React/TypeScript.

## Verification commands

Always run after changes:

```bash
npx tsc --noEmit
cd src-tauri && cargo check
```

Run and build:

```bash
npm run tauri dev
npm run tauri build
```

## Cross-platform delivery

- Put platform-specific dependencies under `[target.'cfg(...)'.dependencies]`, and isolate platform code with `#[cfg(target_os = "...")]`.
- `cargo check` only covers the current target; after modifying a platform-specific branch, do a compile and runtime check on that platform too. Don't mistake a cross-compilation failure caused by a missing target C toolchain for an application bug.
- Windows requires the Rust MSVC toolchain, WebView2 Runtime, and Visual Studio Build Tools (with the C++ workload).
- On macOS, the packaged build and the dev build share the same bundle id. If LaunchServices brings `/Applications/Origami.app` to the foreground, reinstall with the latest `npm run tauri build` output before verifying.

When touching Windows-specific branches, verify in particular:

- `winassoc.rs`: toggling file associations correctly changes and restores Explorer's default program.
- `winmenu.rs`: after installing the classic context menu, Origami launches correctly; after removal, the registry entries are gone.
- `sysauth.rs`: the success, cancel, and unavailable paths for Windows Hello never leak plaintext or lock the user out.
- `passwords.rs`: `passwords.json` only contains id, note, and timestamp; the credential service is `dev.vela.origami.passwords`; deleting an entry also deletes the credential.
- `App.tsx`: `Ctrl + ,` and `Ctrl + +/-/0` work correctly.
- `windows-extension/`: after building, signing, and registering per the README, verify the Win11 new top-level menu.

When touching macOS-specific branches, verify in particular:

- `macassoc.rs`: file association writes, cancellation, and fallback handler behavior are correct.
- `services.rs`: Finder Quick Action install/remove, and compress/extract deep links with correct target paths.
- `sysauth.rs` / `passwords.rs`: system authentication and Keychain read/write, migration, and deletion all work.
- `Cmd + ,` and `Cmd + +/-/0` work correctly.

## Architecture

- **Command registration**: backend commands are defined in `src-tauri/src/lib.rs` and registered in `tauri::generate_handler!`; the frontend wraps them in `src/api.ts`. All three must stay in sync.
- **Platform dispatch**: cross-platform commands expose a unified interface in `lib.rs`, then dispatch to platform modules via `#[cfg]`; unsupported platforms return safe defaults.
- **Window visibility**: the main window starts with `visible: false` and calls `frontend_ready` once the frontend has mounted; quick tasks that need no interaction are driven by the `mini` window, and tasks that need configuration are driven by the `ask` window.
- **Async for heavy work**: blocking tasks like extraction and compression use `spawn_blocking`, coordinating with the frontend through jobs, progress events, and cancel commands.
- **Password gating**: call `system_auth` before revealing plaintext; if authentication is unavailable, allow through; if it fails or the user cancels, don't show it; if the authentication mechanism misbehaves, never lock the user out.
- **Password storage**: plaintext is only ever written to the system credential store; `passwords.json` only stores an index. Legacy plaintext is migrated automatically on load and write.

## Internationalization

- Comments and developer docs are in Chinese; user-facing text must come from resource keys in `src/i18n/locales/` rather than being hardcoded.
- Currently supports `zh-CN` and `en-US`, with `system` as an additional language preference. When adding a new language, extend the resources, the language type, and the settings option together.
- Use stable semantic keys with i18next interpolation/pluralization; don't use the Chinese source text as the key, and don't concatenate translatable sentences in JSX.
- When migrating text, maintain both Chinese and English together; migrating legacy UI in batches is fine, but a given feature area should be migrated completely — avoid mixing Chinese and English within a single dialog.

## Code conventions

- Follow the existing code style and prefer editing existing files; only create new files for resources, platform modules, or when responsibilities are clearly separated.
- "Open with default program" for a single file goes through `extract_entry_to_temp` to extract into `app_cache_dir`, then hands off to the opener.
- Shortcut modifier keys adapt per platform: macOS uses `metaKey`, Windows/Linux use `ctrlKey`.
- Preserve and respect existing changes in the workspace; don't overwrite content unrelated to the current task.
