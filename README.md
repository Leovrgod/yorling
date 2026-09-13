# Yorling

[English](README.md) | [简体中文](README.zh-CN.md)

Yorling is a macOS-first desktop workbench for local AI and productivity workflows. It combines keyboard remapping, agent terminal launching, Finder actions, clipboard history, and a floating agent activity island in one Tauri application.

## Features

- Keyboard workflow layer with macOS-native interception and remapping.
- Multi-terminal launcher for project-specific agent sessions.
- Dynamic Island-style agent activity surface with provider hooks.
- Finder Sync integration for native Finder context-menu actions.
- Clipboard history for text, images, files, folders, and grouped pasteboard items.
- Keyboard music and rhythm tools.

## Tech Stack

- Tauri 2, Rust, Swift, AppKit, and Finder Sync on macOS.
- React 19, TypeScript, Vite, Zustand, and CSS modules for the desktop UI.
- Rust workspace crates for core types, keyboard engine, macOS platform code, island core, and hook bridge.

## Development

Install dependencies:

```sh
pnpm install
```

Run the web UI:

```sh
pnpm dev
```

Run the macOS app development flow:

```sh
pnpm run dev:macos-app
```

Run tests:

```sh
pnpm test
cargo test --workspace
```

Build the frontend:

```sh
pnpm build
```

## macOS Notes

Some features require macOS permissions such as Accessibility, Automation, Screen Recording, and Finder Sync extension approval. The app should guide users to the relevant settings when a feature needs permission.

Finder Sync packaging is handled by:

```sh
pnpm run build:finder-sync
```

## Privacy

Yorling is designed for local desktop workflows. Do not commit local hook exports, personal paths, application support files, logs, tokens, or generated build output.

## License

Yorling is licensed under the MIT License. See [LICENSE](LICENSE).

For macOS keyboard-mouse timing diagnosis, run `bash scripts/diagnose-mouse-macos.sh 30` while moving with the keyboard and then a mouse/trackpad. It records aggregate mouse-event timing only; output goes to the ignored `target/diagnostics/` directory. See [the investigation notes](docs/macos-keyboard-mouse-lag.md) for interpretation and limitations.

Local installers and distribution archives under `app/` and `release/` are excluded from source control. Publish distributable builds as GitHub Release assets.
