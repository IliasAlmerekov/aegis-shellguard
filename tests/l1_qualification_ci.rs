use std::fs;
use std::path::Path;

fn repo_file(path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} must be readable: {err}", path.display()))
}

#[test]
fn every_release_target_builds_the_shipping_binary_in_release_mode() {
    // The build commands live in the `build-target` composite action, which
    // ci.yml and release.yml both call, so CI compiles the same binary a
    // release ships rather than a second copy of the command line.
    let action = repo_file(".github/actions/build-target/action.yml");

    assert!(
        action.contains("cross build --release --target ${{ inputs.target }}"),
        "musl release targets must build the shipping binary in release mode"
    );
    assert!(
        action.contains("cargo build --release --target ${{ inputs.target }}"),
        "macOS release targets must build the shipping binary in release mode"
    );

    let workflow = repo_file(".github/workflows/ci.yml");
    assert!(
        workflow.contains("uses: ./.github/actions/build-target"),
        "release CI must compile every target through the build-target action"
    );
    assert!(
        workflow.contains("include: ${{ fromJSON(needs.gate.outputs.build_targets) }}"),
        "the CI cross matrix must expand .github/build-targets.json rather than \
         listing targets a second time"
    );

    let targets = repo_file(".github/build-targets.json");
    for target in [
        "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-musl",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
    ] {
        assert!(
            targets.contains(target),
            "release CI must retain ADR-022 target `{target}`"
        );
    }
}
