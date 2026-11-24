# Development and releases

## Branches

| Branch | What it's for | CI |
|---|---|---|
| `main` | The development line. Every change lands here through a PR. | Checks; development builds as artifacts |
| `feature/<name>`, `fix/<name>`, … | One piece of work, branched off `main`, merged back by PR. | Checks on the PR |
| `release/vX.Y` | One release series (`X.Y.0`, `X.Y.1`, …), cut from `main`. | Checks; release candidates as artifacts |
| `release/vX` | A major version kept in maintenance while `main` moves on to a big rewrite: minor and patch releases of `X` are tagged on it. Cut from `main`, or from the last `release/vX.Y`. | Checks; release candidates as artifacts |

Tags `vX.Y.Z` mark released commits on a release branch.

**Fixes** can land either way round; both go through a PR:

- **Release branch first**, for a quick patch to a release: open the PR
  against `release/vX.Y` (or `release/vX`), then bring the fix to `main`
  (`git cherry-pick -x`, in a PR of its own) so the next release keeps it.
- **`main` first**, when the fix is part of ongoing work: land it on `main`,
  then cherry-pick it onto the release branch.

Skip carrying a fix to `main` only when `main` has replaced the code it
touches; say so in the PR.

## Versions

The version lives once, in the workspace `Cargo.toml`. A release branch only
builds its own series: `release/v0.2` must carry `0.2.x`, and `release/v1`
`1.x.y`; the build fails otherwise. A tag must match the version on its
commit.

## Cutting a release

1. Branch from `main` and set the version, if `main` doesn't already carry
   it:

   ```sh
   git switch -c release/v0.2 origin/main
   # set version = "0.2.0" in Cargo.toml, add
   # <release version="0.2.0" date="YYYY-MM-DD"/> to the top of <releases>
   # in packaging/linux/io.razza.ServerRemote.metainfo.xml, then:
   cargo check --workspace   # updates Cargo.lock
   git commit -am "chore: release 0.2.0"
   git push -u origin release/v0.2
   ```

2. Open a PR on `main` moving it to the next series (`0.3.0`), so
   development builds aren't mistaken for the release.

## Release candidates

Every push to a release branch runs [Build](../.github/workflows/build.yml),
which uploads one artifact per platform named
`server-remote-<version>-rc.<commit>-<platform>`. Download them from the
run's page, install and test. The run summary shows the commands to tag
that commit.

| Platform | Candidate | Made by |
|---|---|---|
| macOS | `ServerRemote.app`, universal, ad-hoc signed | [`scripts/bundle-macos.sh`](../scripts/bundle-macos.sh), from [`packaging/macos`](../packaging/macos) |
| Windows | A per-user installer (`-setup.exe`) and a portable folder | [Inno Setup](https://jrsoftware.org/isinfo.php), from [`packaging/windows`](../packaging/windows) |
| Linux | An AppImage and a tarball of the binary | [`scripts/bundle-appimage.sh`](../scripts/bundle-appimage.sh), from [`packaging/linux`](../packaging/linux) |

To build packaging changes before they reach `main`, run Build by hand on
the branch (Actions → Build → Run workflow).

The executable's icon comes from `packaging/windows/icon.ico`, which
[`scripts/render-icons.py`](../scripts/render-icons.py) renders from the
logo; rerun it after changing the logo. The installer's `AppId` must never
change, or upgrades install side by side.

The Linux binary links nothing but glibc (the display and GPU libraries
are loaded at run time), so the AppImage bundles no libraries. It's built
on the oldest supported Ubuntu runner, whose glibc sets the oldest
distribution it runs on. The desktop entry and AppStream metadata in
`packaging/linux` are shared with the Flatpak; CI validates both.

The macOS candidate isn't notarised, so Gatekeeper blocks it once
downloaded; clear the quarantine flag to test it
(`xattr -dr com.apple.quarantine ServerRemote.app`). Bundling needs Xcode
26's `actool` for the icon, so the macOS jobs run on `macos-26`.

If testing finds problems, fix them (see **Fixes** above) and merge to the
release branch again; each push makes a new candidate. Artifacts expire with the
repository's retention period (90 days by default), so release a candidate
before then or push again.

## Releasing

Tag the validated candidate's commit and push the tag:

```sh
git tag -a v0.2.0 <commit> -m "ServerRemote 0.2.0"
git push origin v0.2.0
```

[Release](../.github/workflows/release.yml) then:

1. checks the tag matches `Cargo.toml` and sits on `release/v0.2` (or
   `release/v0`, for a major line), and finds
   the successful Build run for that commit; without one, it stops;
2. signs `ServerRemote.app` with the Developer ID certificate, notarises
   and staples it, then puts it in a disk image (with a link to
   `/Applications`) that is signed, notarised and stapled in turn;
3. renames the candidates to `server-remote-<version>-<platform>` (Windows
   as a zip and a `-setup.exe`, Linux as a tarball and an `.AppImage`),
   writes
   `SHA256SUMS`, and drafts a GitHub release with them all attached and
   generated notes.

The binaries are the candidate's, not a rebuild: what was tested ships.
Review the draft and publish it.

Patch releases (`0.2.1`) come from the same branch: merge or cherry-pick fixes, bump
the version (and the metainfo release entry) on the branch, validate the new
candidate, tag.

Windows and Linux builds ship unsigned for now; `SHA256SUMS` lets people
check what they downloaded. SmartScreen warns about the unsigned Windows
installer until it builds up reputation.

## Signing setup

The macOS secrets live in the repository's `release` environment
(Settings → Environments), so only release runs can read them. Restricting
that environment to `v*` tags, or requiring a reviewer, adds a gate before
anything is signed.

| Secret | Value |
|---|---|
| `MACOS_CERT_P12` | Developer ID Application certificate and key, exported as `.p12`, base64-encoded |
| `MACOS_CERT_PASSWORD` | The `.p12` export password |
| `MACOS_SIGNING_IDENTITY` | The certificate's name, e.g. `Developer ID Application: Name (TEAMID)` |
| `APPLE_API_KEY` | App Store Connect API key (`.p8`), base64-encoded, for notarisation |
| `APPLE_API_KEY_ID` | That key's ID |
| `APPLE_API_ISSUER_ID` | The App Store Connect issuer ID |
