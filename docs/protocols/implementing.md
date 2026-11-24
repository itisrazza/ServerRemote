# Implementing a protocol

Many of the protocols ServerRemote targets only have closed first-party
clients, usually written in C, C# (.NET) or Java. Even though ServerRemote is
in Rust, protocol work follows a clean-room process. It has two steps,
done by different hands:

1. **Spec:** someone studies the sources and writes down how the protocol
   works.
2. **Implementation:** someone else implements it from that spec alone.

The spec is the only thing that passes from the first step to the second.

## 1. Write the spec

Each protocol gets a spec page in this folder, named after the protocol
(`ilo4.md`, for example). User-facing setup and caveats stay in the
[protocol notes](README.md).

**Sources**, in order of preference:

1. A published specification (an RFC or the vendor's own documentation). If
   one exists and is complete, the spec page can just link to it and record
   what real servers do differently.
2. Observing a live server: packet captures and black-box experiments.
3. The first-party client: decompiled or disassembled output, its logs and
   its behaviour.
4. Other implementations' source code, whatever their licence.

If a client's licence forbids reverse engineering, ask the maintainer before
using it as a source.

**What goes in:**

- Message layouts, field meanings, byte order, constants and the tables
  needed to interoperate.
- Connection setup, state machines, ciphers and codecs, described in prose,
  tables or short pseudocode written from an understanding of the protocol.
- Where each claim comes from. Mark what has been verified against a live
  server, which model and firmware it was, and when. Keep anything unverified
  under **Open questions**.
- The client's own type and method names, as references for tracing a
  claim back to its source. They aren't names to use in code.

**What stays out:** code copied or transliterated from any source, including
pseudocode that follows the original line by line. Describe what the code
does, not how it's written.

## 2. Implement from the spec

The implementer works only from:

- the spec page and any published specification it links to;
- the behaviour of a real or mock server, observed from the outside;
- the rest of this repository.

They never open the first-party client, its decompile, other
implementations' source, or the tools and notes the spec author made from
them.

**When the spec is wrong or silent,** don't look up the answer in a source.
Add the question to the spec's Open questions, or open an issue, and leave
it to the spec author to answer by updating the spec. Black-box experiments
against a server are fine; record what they show in the spec, so it stays
the one place the protocol is described.

### For coding agents

- Writing the spec and implementing it are **separate sessions**. A session
  that has read a first-party client's code, a decompile or another
  implementation's source writes and fixes specs only. It doesn't write
  connector code for that protocol, even when asked to.
- An implementing session starts fresh, from the spec page. It doesn't read
  the spec session's transcript or the source material, and it doesn't ask
  another session to look things up there.

### For people

If you have studied a protocol's sources, stick to writing and reviewing its
spec rather than implementing it, wherever that's practical.

## Keep sources out of the repository

Never commit:

- first-party client binaries, decompiled or disassembled output, or other
  implementations' source;
- tools or reference decoders written while reading those sources;
- raw packet captures. They hold credentials, session keys and screen
  contents. Commit only test fixtures with the secrets replaced (re-keyed
  with fixed test keys, for example) and nothing identifying the server.

Keep these outside the repository on your own machine, and say where in the
spec, as `ilo4.md` does under Testing during implementation.
