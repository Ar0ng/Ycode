---
name: release-notes
description: Draft user-facing release notes from a git log or a list of changes. Use when the user asks for release notes, a changelog entry or "what's new" text.
license: MIT
---
# Release notes

1. Collect the changes: in Coding mode run `git log --oneline <from>..<to>` (ask for the range if unknown); otherwise use the list the user gave.
2. Group them under **New**, **Improved**, **Fixed**. Drop internal-only changes (CI, refactors) unless the user wants them.
3. Write one line per item in plain language, starting with a verb, no commit hashes.
4. Follow the tone guide in `references/style.md`.
