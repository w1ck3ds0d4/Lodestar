# Contributing

Thanks for helping with Lodestar. Bug reports, fixes and focused features are welcome.

## Development setup

You need the Rust stable toolchain, Node.js 20+, pnpm, and on Windows the WebView2 Runtime plus Visual Studio 2022 Build Tools with the "Desktop development with C++" workload. Lodestar targets Windows 10 and 11 on x64.

```bash
pnpm install
pnpm tauri dev
```

A dev build is unoptimized, so on-device Whisper transcription is much slower than in the release installer.

## Checks

Run these before opening a pull request:

```bash
pnpm build
pnpm test
pnpm format:check

cd src-tauri
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`pnpm format` fixes formatting, and `cargo fmt --all` fixes Rust formatting.

## Commits and pull requests

- Commit messages look like `(type) lowercase summary`, with a type of feat, fix, chore, docs, refactor, revert or test, and no trailing period. Example: `(fix) keep the phone pin check constant time`.
- Keep a pull request to one change, and say what you ran.
- Use plain ASCII hyphens in code, docs and messages.
- Never put API keys, tokens, real resumes or real call transcripts in code, tests, issues or pull requests. Test values must not look like real keys.

## Licensing of contributions

Lodestar is dual-licensed under the AGPL-3.0-or-later and a commercial license (see [COMMERCIAL.md](COMMERCIAL.md)). By submitting a contribution you confirm that you have the right to submit it, that it is licensed to the project under the AGPL-3.0-or-later, and that you also grant WickIT Lab a non-exclusive, perpetual, worldwide, royalty-free license to use, modify and sublicense your contribution, including under its commercial license. You keep the copyright in your contribution.

## Security issues

Do not report vulnerabilities in a public issue or pull request. See [SECURITY.md](SECURITY.md).
