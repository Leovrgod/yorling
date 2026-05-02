# Open Source Readiness Checklist

Use this before making the repository public.

## Required

- Keep `LICENSE` in the repository root.
- Keep `README.md` current with setup, test, build, and permission notes.
- Keep `CONTRIBUTING.md` and `SECURITY.md` available for contributors.
- Confirm `package.json`, Cargo manifests, and release metadata use the same license.
- Run secret scans on the current tree and on Git history.
- Remove local generated files, hook exports, logs, app support data, screenshots with private content, and build artifacts.

## Git History

The `.git` directory itself is not uploaded as a visible folder when pushing to GitHub, but its commit history is what GitHub receives. If old commits contain local paths, reference projects, private notes, secrets, or third-party source snapshots, those old commits can still be viewed after the repo is public.

For the cleanest public launch, create a fresh public repository from the cleaned working tree and make a new initial commit. Keep the current private repository as the development archive.

## Before Publishing

```sh
git status --short
pnpm test
pnpm build
cargo test --workspace
```

If any real secret was ever committed, rotate that credential even if the file is later deleted.
