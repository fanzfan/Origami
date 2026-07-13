type TranslationShape<T> = {
  [K in keyof T]: T[K] extends string ? string : TranslationShape<T[K]>;
};

type ZhCNShape = TranslationShape<typeof import("./zh-CN").zhCN>;

export const enUS = {
  common: {
    cancel: "Cancel",
    close: "Close",
    done: "Done",
    cancelled: "Cancelled",
    preparing: "Preparing…",
  },
  task: {
    quick: "Quick task",
    execute: "Task",
    running: {
      compress: "Compressing…",
      extract: "Extracting…",
    },
    failed: {
      compress: "Compression failed",
      extract: "Extraction failed",
    },
    completed: {
      compress: "Compression complete",
      extract: "Extraction complete",
    },
    finished: "Task complete",
  },
  welcome: {
    reading: "Reading archive…",
    dropHint: "Drop an archive to open it, or drop other files to compress them",
    openArchive: "Open archive…",
    browseFiles: "Browse files…",
    compressFiles: "Compress files…",
    compressFolder: "Compress folder…",
    recent: "Recently opened",
  },
  app: {
    title: {
      thisComputer: "This PC",
      closeArchive: "Close archive",
      returnHome: "Return home",
      home: "Home",
      shellIntegration: "Context menu integration",
      fileAssociations: "File associations",
      passwordManager: "Password manager",
      settings: "Settings",
      settingsShortcutMac: "Settings (⌘,)",
      settingsShortcutOther: "Settings (Ctrl+,)",
      closeWindow: "Close window",
    },
    dialog: {
      archiveFilter: "Archives",
      addFiles: "Choose files to add",
    },
    status: {
      compressing: "Compressing…",
      extracting: "Extracting…",
      adding: "Adding files…",
      removing: "Removing entries…",
      verifying: "Verifying…",
    },
    success: {
      compressedFile: "Compression complete: {{name}}",
      extractedFile: "Extraction complete: {{name}}",
      compressed: "Compression complete",
      extracted: "Extraction complete",
      added_one: "Added {{count}} item",
      added_other: "Added {{count}} items",
      removed_one: "Removed {{count}} item",
      removed_other: "Removed {{count}} items",
      verified: "Verification passed. The archive is intact ✓",
    },
    info: {
      encryptedOpenCancelled: "Opening the encrypted archive was cancelled",
    },
    error: {
      open: "Failed to open: {{message}}",
      browseDirectory: "Failed to open folder: {{message}}",
      compress: "Compression failed: {{message}}",
      extract: "Extraction failed: {{message}}",
      add: "Failed to add files: {{message}}",
      remove: "Failed to remove entries: {{message}}",
      verify: "Verification failed: {{message}}",
    },
  },
  settings: {
    title: "Settings",
    section: {
      general: "General",
      appearance: "Appearance",
      archive: "Compression and extraction",
    },
    language: {
      label: "Language",
      system: "Use system language",
      hint: "The language applies to every Origami window. Screens that have not been migrated yet remain in Chinese.",
    },
    appearanceMode: "Appearance mode",
    mode: {
      system: "System",
      light: "Light",
      dark: "Dark",
    },
    themeLabel: "Theme",
    theme: {
      default: "Origami Blue",
      ocean: "Ocean Blue",
      forest: "Forest Green",
      grape: "Grape Purple",
      rose: "Rose Pink",
      slate: "Slate Gray",
    },
    scale: {
      label: "Interface size: {{percent}}%",
      reset: "Reset",
      hint: "Use Ctrl/⌘ with + or - to resize, and Ctrl/⌘ with 0 to reset.",
    },
    fontLabel: "Font",
    font: {
      system: "System default",
      sans: "Sans serif",
      serif: "Serif",
      mono: "Monospace",
    },
    material: {
      label: "Window material",
      acrylic: "Acrylic",
      mica: "Mica",
      none: "None",
      windowsHint: "On Windows 11, choose Acrylic, Mica, or None. Transparency effects must be enabled in Windows. Unsupported systems stay opaque.",
      opacity: "Acrylic transparency: {{percent}}%",
      opacityHint: "Only Acrylic is adjustable. Move right for more transparency and left for a more solid surface.",
      effect: "Window material effect",
      macHint: "Uses a translucent material on macOS. Unsupported systems stay opaque.",
    },
    level: {
      label: "Default compression level: {{value}}",
      store: "Store only",
      highest: " (highest)",
      storeHint: "Store without compression (fastest)",
      highestHint: "Highest compression ratio (slowest)",
      defaultHint: "Balances compression ratio and speed for new archives.",
    },
    excludeJunk: {
      label: "Exclude system junk files",
      hint: "Automatically skips .DS_Store, __MACOSX, ._ resource forks, Thumbs.db, desktop.ini, and similar files.",
    },
    openAfterExtract: {
      label: "Open the destination folder after extraction",
      hint: "Automatically opens the extracted folder in File Explorer or Finder.",
    },
  },
} as const satisfies ZhCNShape;
