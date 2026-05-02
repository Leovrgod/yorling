# Security Policy

## Reporting a Vulnerability

Please report security issues privately before opening a public issue. If the project has no published security contact yet, contact the maintainer directly and include:

- A clear description of the issue.
- Reproduction steps or a minimal proof of concept.
- The affected platform and app version.
- Any relevant logs with secrets and personal paths removed.

## Sensitive Data

Do not include API keys, signing credentials, access tokens, local hook exports, private transcripts, user clipboard data, or application support files in issues, pull requests, or commits.

## macOS Permissions

Yorling uses macOS capabilities such as Accessibility, Automation, Screen Recording, and Finder Sync. Permission handling should remain explicit, user-controlled, and scoped to the feature that needs it.
