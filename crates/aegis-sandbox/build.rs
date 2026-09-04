//! Build script for `aegis-sandbox`.
//!
//! On Linux, compiles the vendored bubblewrap C sources (ADR-029 §3–§4) into a
//! standalone `bwrap` executable and embeds its bytes into the crate so the
//! runtime can fall back to it when no usable `bwrap` is on `PATH`. Mirrors the
//! Codex reference (`codex-rs/bwrap/build.rs`) but produces the executable
//! directly in `OUT_DIR` rather than via a sibling binary crate, so the embedded
//! binary is always built for the *target* architecture (correct under `cross`).
//!
//! The build is a no-op on non-Linux targets and when `AEGIS_SKIP_BWRAP_BUILD`
//! is set. `AEGIS_BWRAP_SOURCE_DIR` overrides the vendored source directory.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(bwrap_available)");
    println!("cargo:rerun-if-env-changed=AEGIS_BWRAP_SOURCE_DIR");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_ALLOW_CROSS");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_SYSROOT_DIR");
    println!("cargo:rerun-if-env-changed=AEGIS_SKIP_BWRAP_BUILD");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let vendor_dir = manifest_dir.join("vendor/bubblewrap");
    for source in [
        "bubblewrap.c",
        "bind-mount.c",
        "network.c",
        "utils.c",
        "bind-mount.h",
        "network.h",
        "utils.h",
        "VERSION",
    ] {
        println!(
            "cargo:rerun-if-changed={}",
            vendor_dir.join(source).display()
        );
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "linux" || env::var_os("AEGIS_SKIP_BWRAP_BUILD").is_some() {
        return;
    }

    if let Err(err) = try_build_bwrap() {
        panic!("failed to compile bubblewrap for Linux target: {err}");
    }
}

fn try_build_bwrap() -> Result<(), String> {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|err| err.to_string())?);
    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|err| err.to_string())?);
    let src_dir = resolve_bwrap_source_dir(&manifest_dir)?;
    let libcap = pkg_config::Config::new()
        .cargo_metadata(false)
        .probe("libcap")
        .map_err(|err| format!("libcap not available via pkg-config: {err}"))?;

    let config_h = out_dir.join("config.h");
    std::fs::write(
        &config_h,
        r#"#pragma once
#define PACKAGE_STRING "bubblewrap built for Aegis"
"#,
    )
    .map_err(|err| format!("failed to write {}: {err}", config_h.display()))?;

    // Compile the four bubblewrap C files into a static library, renaming the
    // C `main` to `bwrap_main` so a thin Rust-free wrapper can provide the real
    // entry point (mirrors Codex's `-Dmain=bwrap_main`).
    let mut build = cc::Build::new();
    build
        .file(src_dir.join("bubblewrap.c"))
        .file(src_dir.join("bind-mount.c"))
        .file(src_dir.join("network.c"))
        .file(src_dir.join("utils.c"))
        .include(&out_dir)
        .include(&src_dir)
        .define("_GNU_SOURCE", None)
        .define("main", Some("bwrap_main"));
    for include_path in &libcap.include_paths {
        build.flag(format!("-idirafter{}", include_path.display()));
    }
    let compiler = build.get_compiler();
    build.compile("standalone_bwrap");

    // A thin C wrapper that forwards argv to `bwrap_main`. Compiled without the
    // `main` rename so it owns the real entry point.
    let wrapper = out_dir.join("bwrap_main_wrapper.c");
    std::fs::write(
        &wrapper,
        "int bwrap_main(int argc, char **argv);\n\
         int main(int argc, char **argv) { return bwrap_main(argc, argv); }\n",
    )
    .map_err(|err| format!("failed to write {}: {err}", wrapper.display()))?;

    let mut wrapper_build = cc::Build::new();
    wrapper_build
        .file(&wrapper)
        .include(&out_dir)
        .include(&src_dir);
    wrapper_build.compile("bwrap_main_wrapper");

    // Link the two static libraries plus libcap into a standalone `bwrap`
    // executable in `OUT_DIR`, which the crate embeds via `include_bytes!`.
    let bwrap_path = out_dir.join("bwrap");
    let mut cmd = Command::new(compiler.path());
    cmd.arg("-o").arg(&bwrap_path);
    // The wrapper defines `main` (the entry point) and references `bwrap_main`,
    // so it must come first: the linker pulls the wrapper object, then pulls
    // the `bwrap_main` object from the standalone archive. Reversed, the
    // standalone object is never pulled and `bwrap_main` stays undefined.
    cmd.arg(out_dir.join("libbwrap_main_wrapper.a"));
    cmd.arg(out_dir.join("libstandalone_bwrap.a"));
    for link_path in &libcap.link_paths {
        cmd.arg(format!("-L{}", link_path.display()));
    }
    for lib in &libcap.libs {
        cmd.arg(format!("-l{}", lib));
    }
    // musl targets must produce a fully static binary; the musl compiler links
    // statically by default, but be explicit so the assertion below is honest.
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("musl") {
        cmd.arg("-static");
    }
    let status = cmd
        .status()
        .map_err(|err| format!("failed to link bwrap: {err}"))?;
    if !status.success() {
        return Err(format!("linker failed with status {status}"));
    }

    // Build-time static assertion (ADR-029 §3): the embedded bwrap must be
    // statically linked on musl targets, or it would fail at runtime on a musl
    // host. `readelf` is architecture-agnostic, so this also holds under cross.
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("musl") {
        assert_static(&bwrap_path)?;
    }

    println!("cargo:rustc-cfg=bwrap_available");
    Ok(())
}

/// Verify a produced ELF has no dynamic interpreter and no shared-library
/// dependencies — the same check `release.yml` applies to the Aegis binary.
fn assert_static(binary: &Path) -> Result<(), String> {
    let readelf = Command::new("readelf")
        .arg("-l")
        .arg(binary)
        .output()
        .map_err(|err| format!("failed to run readelf on {}: {err}", binary.display()))?;
    let out = String::from_utf8_lossy(&readelf.stdout);
    if out.contains("INTERP") {
        return Err(format!(
            "{} is dynamically linked (PT_INTERP present); expected static",
            binary.display()
        ));
    }

    let readelf_d = Command::new("readelf")
        .arg("-d")
        .arg(binary)
        .output()
        .map_err(|err| format!("failed to run readelf -d on {}: {err}", binary.display()))?;
    let out_d = String::from_utf8_lossy(&readelf_d.stdout);
    if out_d.contains("NEEDED") {
        return Err(format!(
            "{} has shared library dependencies (DT_NEEDED); expected static",
            binary.display()
        ));
    }
    Ok(())
}

/// Resolve the bubblewrap source directory used for build-time compilation.
///
/// Priority:
/// 1. `AEGIS_BWRAP_SOURCE_DIR` points at an existing bubblewrap checkout.
/// 2. The vendored bubblewrap tree under `crates/aegis-sandbox/vendor/bubblewrap`.
fn resolve_bwrap_source_dir(manifest_dir: &Path) -> Result<PathBuf, String> {
    if let Ok(path) = env::var("AEGIS_BWRAP_SOURCE_DIR") {
        let src_dir = PathBuf::from(path);
        if src_dir.exists() {
            return Ok(src_dir);
        }
        return Err(format!(
            "AEGIS_BWRAP_SOURCE_DIR was set but does not exist: {}",
            src_dir.display()
        ));
    }

    let vendor_dir = manifest_dir.join("vendor/bubblewrap");
    if vendor_dir.exists() {
        return Ok(vendor_dir);
    }

    Err(format!(
        "expected vendored bubblewrap at {}, but it was not found.\n\
Set AEGIS_BWRAP_SOURCE_DIR to an existing checkout or vendor bubblewrap under \
crates/aegis-sandbox/vendor.",
        vendor_dir.display()
    ))
}
