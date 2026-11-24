# Contributing

Thanks for helping with ServerRemote. It's early and experimental, so
interfaces move often; open an issue before starting anything large so we
can agree on the shape first.

## Reporting problems

Use the [issue forms](https://github.com/itisrazza/ServerRemote/issues/new/choose).
Reports from real hardware are especially useful: much of the iLO and Spice
work was built from protocol analysis and simulators, and the
[protocol notes](../docs/protocols/README.md) say what hasn't been verified.

Security problems go through the [security policy](SECURITY.md), not issues.

## Getting started

The [README](../README.md) covers building, the workspace layout and
translations. The [architecture docs](../docs/architecture/README.md) explain
how connectors, sessions and the capability-driven UI fit together.
[AGENTS.md](../AGENTS.md) holds the project's rules (UI-free core, theme
colours, `fl!` strings, protocol traps); they apply to people as much as to
coding agents.

Protocol work is clean-room: a spec first, then an implementation written
from that spec alone. Read
[implementing a protocol](../docs/protocols/implementing.md) before working
on a connector.

## Making a change

1. Branch off `main` (`feature/<name>`, `fix/<name>`, …) and open a pull
   request back into it. [Releasing](../docs/releasing.md) explains how
   release branches and fixes to them work.
2. Write commit messages as
   [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/),
   scoped where it helps: `feat(ui): …`, `fix(tls): …`, `docs: …`.
3. Run the checks CI runs:

   ```sh
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

4. Update the docs in the same change when a feature lands or its status
   changes. The [agent guide](../AGENTS.md#checks) lists which files.

## Licence

ServerRemote is dual-licensed under MIT and Apache 2.0. Unless you say
otherwise, any contribution you submit is licensed the same way, with no
additional terms or conditions.
