// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The hosted posture travels INSIDE the image (#423): `docker/Dockerfile`
//! bakes the compose file and the Caddyfile the hosted box runs, and the one
//! command that box's deploy key may run extracts them from the image it just
//! pulled. Two files, three places, and the paths must agree — a Dockerfile
//! COPY nobody reads is a posture change that never arrives.
//!
//! What these read is the BUILD INSTRUCTION and the deploy script's text.
//! Whether a built image carries the files belongs to the lane that builds one,
//! and whether the extraction works on the box belongs to a real deploy. The
//! shell itself is linted by `scripts/checks/hosted-deploy-script.sh`.
//!
//! Feature-independent on purpose: everything here is a committed file, so the
//! module carries its own repository root rather than riding the `ssr` gate the
//! engine-gate helper sits behind.

use std::path::{Path, PathBuf};

/// The repository root, from this crate's manifest directory.
fn repo_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The posture files `docker/Dockerfile` COPYs into the image, as
/// `(build-context source, container path)` pairs.
///
/// READ out of the Dockerfile, because a constant restating the paths here
/// would prove nothing about what the image ships.
fn baked_posture() -> Result<Vec<(PathBuf, String)>, Box<dyn std::error::Error>> {
    let dockerfile = std::fs::read_to_string(repo_root().join("docker/Dockerfile"))?;
    Ok(dockerfile
        .lines()
        .filter_map(|line| line.trim().strip_prefix("COPY "))
        .filter_map(|rest| {
            let mut words = rest
                .split_whitespace()
                .filter(|word| !word.starts_with("--"));
            let source = words.next()?;
            let destination = words.next()?;
            destination
                .contains("/posture/")
                .then(|| (PathBuf::from(source), destination.to_owned()))
        })
        .collect())
}

/// The `deploy.sh` cloud-init writes to the hosted box, as text.
///
/// The whole cloud-init document: the script is the only place in it that names
/// a container path, so a plain containment assertion over the document is a
/// true statement about the script.
fn cloud_init() -> Result<String, Box<dyn std::error::Error>> {
    Ok(std::fs::read_to_string(
        repo_root().join("deploy/hosted/cloud-init.yaml"),
    )?)
}

/// The image bakes both posture files, under `/app` where nothing mounts over
/// them, from the committed `deploy/hosted/` sources — and the deploy script
/// copies them out of exactly those paths.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_image_bakes_the_posture_the_hosted_deploy_extracts() -> Result<(), Box<dyn std::error::Error>>
{
    let baked = baked_posture()?;
    let mut destinations: Vec<&str> = baked
        .iter()
        .map(|(_, destination)| destination.as_str())
        .collect();
    destinations.sort_unstable();
    assert_eq!(
        destinations,
        vec!["/app/posture/Caddyfile", "/app/posture/docker-compose.yml"],
        "the image must carry exactly the two files the hosted deploy installs"
    );

    let deploy = cloud_init()?;
    for (source, destination) in &baked {
        assert!(
            destination.starts_with("/app/"),
            "a posture file under /work would be shadowed by an operator's bind mount: {destination}"
        );
        assert!(
            source.starts_with("deploy/hosted/"),
            "the baked posture must be the committed hosted posture, not a second copy: {}",
            source.display()
        );
        let in_context = repo_root().join(source);
        assert!(
            in_context.is_file(),
            "the build context has no {}, so that COPY would fail the build",
            in_context.display()
        );
        assert!(
            deploy.contains(destination.as_str()),
            "nothing on the hosted box reads {destination}, so a change to it would never arrive"
        );
    }

    assert!(
        deploy.contains("docker create"),
        "the distroless image carries no shell, so the extraction must create a container and copy out of it"
    );
    Ok(())
}

/// The hosted Caddyfile keeps `no-cache` over the whole bundle as the floor,
/// and the `immutable` override for content-hashed names is written AFTER it.
///
/// Order is the correctness property (#450): directives carrying named
/// matchers keep their Caddyfile order
/// (<https://caddyserver.com/docs/caddyfile/directives#sorting-algorithm>), so
/// the override written first would be overwritten by the floor, and every
/// hashed asset would revalidate on every load.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_bundle_floor_is_no_cache_and_the_hashed_override_follows_it()
-> Result<(), Box<dyn std::error::Error>> {
    let caddyfile = std::fs::read_to_string(repo_root().join("deploy/hosted/Caddyfile"))?;

    let floor = caddyfile
        .find("header @bundle Cache-Control \"no-cache\"")
        .ok_or(
            "the /pkg floor must stay: a hashed build that regresses degrades to revalidation",
        )?;
    let override_at = caddyfile
        .find("header @hashed Cache-Control \"public, max-age=31536000, immutable\"")
        .ok_or("a content-hashed name never names other bytes, so it is served immutable")?;
    assert!(
        floor < override_at,
        "the immutable override must be written after the no-cache floor, or the floor overwrites it"
    );

    assert!(
        caddyfile.contains("@bundle path /pkg/*"),
        "the floor must cover everything under /pkg, hashed or not"
    );
    assert!(
        caddyfile.contains("@hashed path_regexp"),
        "the override must select on the emitted <name>.<hash>.<ext> shape, never on all of /pkg"
    );
    Ok(())
}

/// `.dockerignore` excludes none of the posture: an excluded path is a COPY
/// that fails the build, and the exclusion is invisible in the Dockerfile.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_build_context_ignore_file_keeps_the_posture() -> Result<(), Box<dyn std::error::Error>> {
    let ignore = std::fs::read_to_string(repo_root().join(".dockerignore"))?;
    for line in ignore.lines() {
        let pattern = line.trim();
        if pattern.is_empty() || pattern.starts_with('#') || pattern.starts_with('!') {
            continue;
        }
        assert!(
            !pattern.starts_with("deploy"),
            "`{pattern}` keeps the hosted posture out of the build context, and the COPY of it would fail"
        );
    }
    Ok(())
}
