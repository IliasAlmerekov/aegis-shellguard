#![no_main]

use libfuzzer_sys::fuzz_target;

// Exercises the public Bash adapter on arbitrary shell-like source.
fuzz_target!(|data: &[u8]| {
    let source = String::from_utf8_lossy(data);
    let _ = aegis_language::languages::bash::analyze(source.as_ref());
});
