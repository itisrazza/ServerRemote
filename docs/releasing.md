# Development and releases

## Branches

| Branch | What it's for | CI |
|---|---|---|
| `main` | The development line. Every change lands here through a PR. | Checks; development builds as artifacts |
| `feature/<name>`, `fix/<name>`, … | One piece of work, branched off `main`, merged back by PR. | Checks on the PR |
| `release/vX.Y` | Stabilising one release series, cut from `main`. | Checks; release candidates as artifacts |

Tags `vX.Y.Z` mark released commits on a release branch.

Fixes go **upstream first**: land the fix on `main` through a PR, then
cherry-pick it onto the release branch (`git cherry-pick -x`). A fix that
only makes sense on the release branch (it touches code `main` has since
replaced) can go straight to a PR against that branch.

## Versions

The version lives once, in the workspace `Cargo.toml`. A release branch only
builds its own series: `release/v0.2` must carry `0.2.x`, and the build fails
otherwise.

## Cutting a release

1. Branch from `main` and set the version, if `main` doesn't already carry
   it:

   ```sh
   git switch -c release/v0.2 origin/main
   # set version = "0.2.0" in Cargo.toml, then:
   cargo check --workspace   # updates Cargo.lock
   git commit -am "chore: release 0.2.0"
   git push -u origin release/v0.2
   ```

2. Open a PR on `main` moving it to the next series (`0.3.0`), so
   development builds aren't mistaken for the release.

## Release candidates

Every push to `release/vX.Y` runs [Build](../.github/workflows/build.yml),
which uploads one artifact per platform named
`server-remote-<version>-rc.<commit>-<platform>`. Download them from the
run's page, install and test. The run summary shows the commands to tag
that commit.

| Platform | Candidate | Made by |
|---|---|---|
| macOS | `ServerRemote.app`, universal, ad-hoc signed | [`scripts/bundle-macos.sh`](../scripts/bundle-macos.sh), from [`packaging/macos`](../packaging/macos) |
| Windows | A per-user installer (`-setup.exe`) and a portable folder | [Inno Setup](https://jrsoftware.org/isinfo.php), from [`packaging/windows`](../packaging/windows) |
| Linux | `server-remote` | `cargo build` |

To build packaging changes before they reach `main`, run Build by hand on
the branch (Actions → Build → Run workflow).

The executable's icon comes from `packaging/windows/icon.ico`, which
[`scripts/render-icons.py`](../scripts/render-icons.py) renders from the
logo; rerun it after changing the logo. The installer's `AppId` must never
change, or upgrades install side by side.

The macOS candidate isn't notarised, so Gatekeeper blocks it once
downloaded; clear the quarantine flag to test it
(`xattr -dr com.apple.quarantine ServerRemote.app`). Bundling needs Xcode
26's `actool` for the icon, so the macOS jobs run on `macos-26`.

If testing finds problems, fix them (upstream first) and push to the release
branch again; each push makes a new candidate. Artifacts expire with the
repository's retention period (90 days by default), so release a candidate
before then or push again.

## Releasing

Tag the validated candidate's commit and push the tag:

```sh
git tag -a v0.2.0 <commit> -m "ServerRemote 0.2.0"
git push origin v0.2.0
```

[Release](../.github/workflows/release.yml) then:

1. checks the tag matches `Cargo.toml` and sits on `release/v0.2`, and finds
   the successful Build run for that commit; without one, it stops;
2. signs `ServerRemote.app` with the Developer ID certificate, notarises
   and staples it, then puts it in a disk image (with a link to
   `/Applications`) that is signed, notarised and stapled in turn;
3. renames the candidates to `server-remote-<version>-<platform>` (Windows
   as a zip and a `-setup.exe`), writes
   `SHA256SUMS`, and drafts a GitHub release with them all attached and
   generated notes.

The binaries are the candidate's, not a rebuild: what was tested ships.
Review the draft and publish it.

Patch releases (`0.2.1`) come from the same branch: cherry-pick fixes, bump
the version on the branch, validate the new candidate, tag.

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
