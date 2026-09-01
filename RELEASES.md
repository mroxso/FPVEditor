# Releases

Releases use [Release Please](https://github.com/googleapis/release-please).

- Merge conventional commits into `main` (`fix:` creates a patch release,
  `feat:` a minor release, and `feat!:` / `BREAKING CHANGE:` a major release).
- Release Please keeps one release pull request up to date, including the
  workspace Cargo version and Tauri bundle version.
- Merge that release pull request to tag and publish it. GitHub Actions then
  attaches macOS ARM64, Windows x64, and Linux x64 bundles to the GitHub
  release.

The first release needs one conventional commit after this automation has been
merged (for example `fix: configure release tags`). That creates the first
release PR. Afterwards the manifest and the `vX.Y.Z` tag track each release.

This intentional release-PR gate avoids publishing a new version for every
documentation or CI-only merge while keeping normal releases one merge away.
