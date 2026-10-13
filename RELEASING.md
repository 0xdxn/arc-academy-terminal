# Releasing arct

Releases are deliberate. Nothing here runs on its own except the GitHub build, which only produces a draft.

1. **Prepare** on `main` (gitlab first):
   - bump `version` in the root `Cargo.toml` and the `arct-*` path dependencies in `crates/*/Cargo.toml`;
   - set `VERSION` in `install.sh` to the same version;
   - move the `CHANGELOG.md` entry from "unreleased" to today's date;
   - `cargo test --workspace` and play the first Explorer lesson by hand.
2. **Tag**: `git tag vX.Y.Z && git push gitlab vX.Y.Z`. Pushing the same tag to GitHub runs `.github/workflows/release.yml`, which builds the four archives (macOS arm64 and x86_64, Linux x86_64 and arm64), writes `SHA256SUMS`, and opens a **draft** release.
3. **Check the draft**: download one archive for your platform, compare its hash with `SHA256SUMS`, run `arct --version`.
4. **Publish downloads** to `https://arcacademy.sh/releases/vX.Y.Z/` (the archives plus `SHA256SUMS`; the site serves them, and `install.sh` downloads from there). On hub that is `/home/arc/arc-academy-releases/vX.Y.Z/` (see arc-academy-web DEPLOY.md). Only version directories, archives and `SHA256SUMS` ever go in that directory, and never a symlink: no `latest` link, and copy with `rsync -rt` or `cp`, not `rsync -a` from a tree that has links. A `latest` pointer, if ever wanted, is a real copy or a redirect route. Check every hash on hub (`sha256sum -c SHA256SUMS`) before fetching any of the URLs. Then publish the GitHub draft.
5. **crates.io** (optional, needs `cargo login`), in dependency order: `arct-core`, then `arct-config`, `arct-telemetry`, `arct-ai`, then `arct-tui`, then `arct-cli`.
6. **Test the installer** on a clean machine or account: `curl -fsSL https://arcacademy.sh/install.sh | sh`.

Never replace the files of a published version: if something is wrong, release the next patch version.

Local packaging for one platform, without CI: `scripts/package.sh [target]` writes `dist/`.
