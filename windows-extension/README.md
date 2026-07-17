# Origami Windows Context Menu

Windows has two right-click menu paths, corresponding to two menu forms:

| Form | Implementation | Status | Notes |
| --- | --- | --- | --- |
| **Classic menu** (Win11 "Show more options" / Shift+F10; direct top-level on Win10) | Registry `HKCU\Software\Classes` (`src-tauri/src/winmenu.rs`) | **Wired into the main app** | One-click install/remove via the app's "Context Menu Integration", no signed package needed |
| **Win11 new top-level menu** | IExplorerCommand COM handler + MSIX sparse package in this directory | **Can be developed and registered, pending release signing** | Must be signed and the sparse package registered |

The classic menu is already good enough for daily use; this directory is the implementation for going further into the Win11 "new top-level menu".

## Why the new top-level menu requires packaging

Win11's new right-click menu **only** accepts an `IExplorerCommand` handler provided by a
packaged app (MSIX/sparse package), and the package must be signed with a certificate
trusted by the machine. This is the same kind of platform restriction as macOS requiring
Developer ID + notarization for a top-level Finder extension. An unpackaged Tauri app can
only write to the classic registry menu.

## Directory contents

- `explorer-command/` — an in-process COM server (cdylib) implementing five pinned
  top-level commands: ZIP, 7Z, detailed compress, smart extract, and extract-to-location.
  On invoke, it launches `Origami.exe --compress=<zip|7z|ask>` or `--extract=<smart|ask>`
  from the same directory.
- `AppxManifest.xml` — the sparse package manifest: declares the COM server and hooks it
  into `windows.fileExplorerContextMenus`.
- `build.ps1` — builds the DLL on Windows, self-signs it, and registers it via
  ExternalLocation (for development).

## Development setup steps (Windows)

1. Install Origami and note its install directory (containing `Origami.exe`).
2. Turn on "Developer Mode" (Settings › Privacy & security › For developers).
3. Run:
   ```powershell
   cd windows-extension
   ./build.ps1 -AppDir "C:\Program Files\Origami"
   ```
4. Restart Explorer: `Stop-Process -Name explorer -Force; Start-Process explorer`
5. Right-click a file/folder → three compress commands appear at the top level; right-click a supported archive and two extract commands also appear.

Uninstall: `Get-AppxPackage *Origami.ShellExtension* | Remove-AppxPackage`

## Notes

- The COM code is written against the `windows` crate 0.58 interface, and **already builds
  successfully on Windows with `cargo build --release`** (requires the `implement` feature
  of `windows`; in 0.58 the shell item parameters of methods like `IExplorerCommand` are
  `Option<&IShellItemArray>`, the outer parameter of `IClassFactory::CreateInstance` is
  `Option<&IUnknown>`, and `IEnumExplorerCommand::Skip`/`Reset` return `Result<()>`). These
  signatures may need tweaking again when changing crate versions. Runtime behavior (the
  top-level menu actually appearing and correctly launching Origami.exe) still needs to be
  verified on a device after registration.
- The `<Identity Publisher=...>` in `AppxManifest.xml` must exactly match the signing
  certificate's Subject.
- The `CLSID` must match between `explorer-command/src/lib.rs` and `AppxManifest.xml`.
- Distributing to others requires re-signing with a trusted Authenticode certificate — a
  self-signed certificate won't work.
