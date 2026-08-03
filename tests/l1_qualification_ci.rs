//! Release-CI contracts for the ADR-022 L1 qualification gate.

use std::fs;
use std::path::Path;

fn ci_workflow() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/ci.yml");
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} must be readable: {err}", path.display()))
}

#[test]
fn every_release_target_builds_the_shipping_binary_in_release_mode() {
    // ADR-022 §8: every qualified grammar must be statically present in every
    // official release binary. Compiling only aegis-language tests proves the
    // crate links, not that the binary selected for release retains the set.
    let workflow = ci_workflow();

    assert!(
        workflow.contains("cross build --release --target ${{ matrix.target }}"),
        "musl release targets must build the shipping binary in release mode"
    );
    assert!(
        workflow.contains("cargo build --release --target ${{ matrix.target }}"),
        "macOS release targets must build the shipping binary in release mode"
    );
    for target in [
        "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-musl",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
    ] {
        assert!(
            workflow.contains(target),
            "release CI must retain ADR-022 target `{target}`"
        );
    }
}
