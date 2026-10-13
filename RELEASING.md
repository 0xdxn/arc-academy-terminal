# Releasing arct

Releases are deliberate. Nothing here runs on its own except the GitHub build, which only produces a draft.

1. **Prepare** on `main` (gitlab first):
   - bump `version` in the root `Cargo.toml` and the `arct-*` path dependencies in `crates/*/Cargo.toml`;
   - set `VERSION` in `install.sh` to the same version;
   - move the `CHANGELOG.md` entry from "unreleased" to today's date;
   - `cargo test --workspace` and play the first Explorer lesson by hand;
   - on a Linux machine with bubblewrap (hub), `ARCT_REQUIRE_JAIL=1 cargo test --workspace --locked --no-fail-fast` and keep its count and exit code with the release record. With `ARCT_REQUIRE_JAIL` set, a jail that doesn't work fails the tests instead of skipping them. The release workflow runs the same on macOS and Linux; keep this step until a release has shown the Linux jail passing there.
2. **Tag**: `git tag vX.Y.Z && git push gitlab vX.Y.Z`. Pushing the same tag to GitHub runs `.github/workflows/release.yml`, which builds the four archives (macOS arm64 and x86_64, Linux x86_64 and arm64), writes `SHA256SUMS`, and opens a **draft** release.
3. **Check the draft**: download one archive for your platform, compare its hash with `SHA256SUMS`, run `arct --version`.
4. **Publish downloads** to `https://arcacademy.sh/releases/vX.Y.Z/` (the archives plus `SHA256SUMS`; the site serves them, and `install.sh` downloads from there). On hub that is `/home/arc/arc-academy-releases/vX.Y.Z/` (see arc-academy-web DEPLOY.md). The rule: nothing but version directories, archives and `SHA256SUMS` ever goes in that directory. Two ways to break it without noticing are links: a symlink (the site refuses one that points outside, but don't rely on that) and a hard link (`cp -l`, `rsync -H`), which is a second name for a file elsewhere and the site cannot tell apart from a real file. So copy with `rsync -rt` or plain `cp`, never `rsync -a`/`-H` or `cp -l`; a `latest` pointer, if ever wanted, is a real copy or a redirect route. On hub, `find <dir> -type l` and `find <dir> -type f -links +1` must both print nothing, and `sha256sum -c SHA256SUMS` must pass, before fetching any of the URLs. Then publish the GitHub draft.
5. **crates.io** (optional, needs `cargo login`), in dependency order: `arct-core`, then `arct-config`, `arct-telemetry`, `arct-ai`, then `arct-tui`, then `arct-cli`.
6. **Test the installer** on a clean machine or account: `curl -fsSL https://arcacademy.sh/install.sh | sh`.

Never replace the files of a published version: if something is wrong, release the next patch version.

Local packaging for one platform, without CI: `scripts/package.sh [target]` writes `dist/`.
