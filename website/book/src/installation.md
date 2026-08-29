# Installation

There are four ways to get the command, and all four end at the same place: a
`veredictum` you can point at a catalogue. Pick by what you already have
installed. Whichever you pick, you also want a clone of the repository, and the
first section says why.

<!-- toc -->

## What you need, and why a clone is part of it

The instrument reads three things at run time: the catalogue, the vendored
specification oracle, and your own declaration of the deployment under test. It
reads all three as paths you pass on the command line, so the code and the data
travel separately.

That split is deliberate. The catalogue and the specification oracle together
are over 300 MB of data, which no package registry accepts, and a party may
legitimately want to point the instrument at a catalogue of their own. So the
published crate and the published image carry the code, and a clone of the
repository is where the data lives:

```bash
git clone https://github.com/rubentalstra/Veredictum
cd Veredictum
```

Pick one of the three ways of getting the command below. All of them then run
the same subcommands against that clone.

## With cargo

The binary is on [crates.io](https://crates.io/crates/veredictum). Take this
path if you want the command on your `PATH`.

```bash
cargo install veredictum --locked
veredictum validate --root artifacts --specs specs/openehr
```

Two flags are worth understanding rather than copying:

- `--locked` builds against the `Cargo.lock` the release was tested with. Leave
  it off and cargo resolves fresh versions of every dependency, which is a
  different build from the one the project's gates ran.

The library target is published with the binary, so you can consume the typed
artifact model and the published JSON Schemas directly rather than
reimplementing the format.

## With Docker: the web console

The container image is the web console: a browser frontend over the same
instrument, served by its own binary. The CLI is deliberately not distributed
as an image — a static binary needs no container, and the release binaries
below are its no-toolchain path. Start the console against a clone and it
serves on port 3000:

```bash
docker run --rm -p 127.0.0.1:3000:3000 -v "$PWD:/work" \
    ghcr.io/rubentalstra/veredictum:<tag>
```

Substitute a published tag from the
[package page](https://github.com/rubentalstra/Veredictum/pkgs/container/veredictum)
for `<tag>`. The image is multi-architecture and is pushed by digest, with its
tags applied only after a smoke run and a vulnerability scan of that digest have
passed.

The catalogue and the specification oracle are not baked into the image. That is
the same over 300 MB reason as above, and it means the data you grade against is the
data you can see in your own checkout. The console has no login, so the
publish flag above binds it to loopback; exposing it further is the
operator's decision, behind their own gate.

> [!NOTE]
> Every image tag published so far predates the console's first release and
> still carries the CLI as the payload, invoked as
> `docker run --rm -v "$PWD:/work" ghcr.io/rubentalstra/veredictum:<tag>
> validate --root /work/artifacts --specs /work/specs/openehr`. The console
> serves from its first release tag onward; [the console
> chapter](console.md) shows what it does today.

## From a release binary

Prebuilt binaries for `x86_64` and `aarch64` Linux are attached to every
[release](https://github.com/rubentalstra/Veredictum/releases). Each tarball
ships with a `sha256sum`, a CycloneDX dependency SBOM and a Sigstore bundle.

Verify the bundle before you run the binary. The check that matters is not just
"this file is signed" but "this file was built by the workflow in this
repository", which is what `--signer-workflow` asserts:

```bash
gh attestation verify veredictum-<tag>-<target>.tar.gz \
    -R rubentalstra/Veredictum \
    --signer-workflow rubentalstra/Veredictum/.github/workflows/release-build.yml
```

A release is created as a draft and published only once every expected asset is
attached, so you never meet a release whose binaries are still uploading. Its
tag is signed, and a repository rule refuses to delete one, so a tag is never
re-pointed at different code. The recovery path for a bad cut is the next
version.

## From source

The toolchain pins itself from `rust-toolchain.toml`, so you do not choose a
Rust version:

```bash
cargo run -- validate --root artifacts --specs specs/openehr
```

The declared minimum supported Rust version is 1.96, verified in CI against
that floor rather than assumed. The only extra tool is `cargo-nextest`, and only
if you intend to run the project's own test suite.

## Checking that the install works

`validate` is the check to run first, because it needs no server:

```bash
veredictum validate --root artifacts --specs specs/openehr
```

A working install over an intact clone prints one line and exits zero:

```text
1130 case(s), 249 binding(s), 2 party statement(s), 0 finding(s)
```

Any finding count above zero is a failure of the catalogue, not of your setup,
and the findings printed above that line say which artifact is at fault. A
missing `--specs` tree is the common first-run mistake: the citation and
Service-Model gates are skipped without it, so the case count is reported while
the checks that make the count meaningful never run.
