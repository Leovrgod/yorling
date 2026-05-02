# Contributing

Thanks for helping improve Yorling.

## Development Guidelines

- Keep changes focused on the feature or bug being addressed.
- Prefer existing module patterns before adding new abstractions.
- Keep macOS-native behavior in mind for Finder, keyboard, terminal, and window-management flows.
- Add or update tests when behavior changes.
- Avoid committing local paths, logs, generated output, personal hook configs, app data, or secrets.

## Checks

Before opening a pull request, run the relevant checks:

```sh
pnpm test
pnpm build
cargo test --workspace
```

For macOS integration changes, also validate the packaged app path when possible:

```sh
pnpm run dev:macos-app
```

## Comments

Comments should explain project behavior, platform constraints, or non-obvious implementation details. Remove personal notes, temporary debugging comments, and local-environment references before committing.
