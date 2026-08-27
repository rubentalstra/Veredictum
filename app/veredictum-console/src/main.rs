// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console server binary (`ssr` feature): serves the Leptos routes, the
//! compiled client bundle, and a `/healthz` liveness endpoint. A featureless
//! build compiles a stub so plain `cargo build --all-targets` over the
//! workspace stays green; the shipped binary is always the `ssr` shape
//! (`bin-features` in `Cargo.toml`).
//!
//! Container duties live here because the image is distroless: the binary is
//! PID 1 (exec-form ENTRYPOINT), so it must handle SIGTERM itself for
//! `docker stop` to end it gracefully instead of by SIGKILL after the grace
//! period, and it doubles as its own health probe (`veredictum-console
//! healthcheck`) because the image carries no shell and no curl.

/// Serves the console on the configured address (`site-addr`, overridable
/// through the standard `LEPTOS_SITE_ADDR` environment variable), or, when
/// invoked as `veredictum-console healthcheck`, probes the running server's
/// `/healthz` once and exits by the outcome.
///
/// # Errors
/// Returns an error when the Leptos configuration cannot be read, the
/// listener cannot bind, the server stops on a fault, or — in healthcheck
/// mode — the probe does not answer healthy.
#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use leptos::prelude::get_configuration;
    use leptos_axum::{LeptosRoutes, generate_route_list};
    use veredictum_console::app::{App, shell};

    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        return healthcheck();
    }

    let conf = get_configuration(None)?;
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    let app = axum::Router::new()
        // The liveness endpoint the container HEALTHCHECK and any
        // orchestrator probe read. Deliberately outside the Leptos route
        // tree: it must answer even if the WASM bundle or the app shell is
        // broken, because it claims only "the server accepts connections".
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .leptos_routes(&leptos_options, routes, {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Resolves when the process receives SIGTERM (what `docker stop` and every
/// orchestrator send PID 1) or SIGINT (Ctrl-C in a terminal), so axum drains
/// in-flight requests instead of dying by SIGKILL at the grace-period
/// deadline.
#[cfg(feature = "ssr")]
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        // No signal handler means no graceful path exists; pending forever
        // leaves shutdown to the runtime's own termination.
        let Ok(mut sigterm) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        else {
            return std::future::pending().await;
        };
        tokio::select! {
            _ = sigterm.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Probes the running server's `/healthz` over one plain HTTP/1.1 exchange.
///
/// Hand-rolled over `std::net::TcpStream` on purpose: the probe runs inside
/// the distroless image where no curl exists, and pulling an HTTP client into
/// the binary for one localhost GET would be the heavier tool. `Connection:
/// close` keeps the read finite.
///
/// # Errors
/// Returns an error when the connection, the write, the read, or the status
/// line fails — each is an unhealthy verdict, and the non-zero exit is what
/// the container HEALTHCHECK counts.
#[cfg(feature = "ssr")]
fn healthcheck() -> anyhow::Result<()> {
    use std::io::{Read, Write};

    let addr = std::env::var("LEPTOS_SITE_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:3000"));
    // The server binds 0.0.0.0 in the container; the probe connects to the
    // loopback realization of that bind.
    let addr = addr.replace("0.0.0.0", "127.0.0.1");
    let timeout = std::time::Duration::from_secs(3);

    let target = std::net::ToSocketAddrs::to_socket_addrs(addr.as_str())?
        .next()
        .ok_or_else(|| anyhow::anyhow!("LEPTOS_SITE_ADDR resolves to no address: {addr}"))?;
    let mut stream = std::net::TcpStream::connect_timeout(&target, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    if response.starts_with("HTTP/1.1 200") {
        Ok(())
    } else {
        let status = response.lines().next().unwrap_or("no response");
        anyhow::bail!("unhealthy: {status}")
    }
}

// The featureless stub: the client entry point is `lib.rs::hydrate`, and the
// server shape is selected by cargo-leptos; this exists only so the bin
// target compiles under every feature set the workspace gates build.
#[cfg(not(feature = "ssr"))]
fn main() {}
