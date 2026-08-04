#![no_main]

use libfuzzer_sys::fuzz_target;

// Exercises the public TypeScript adapter, including error recovery paths.
fuzz_target!(|data: &[u8]| {
    let source = String::from_utf8_lossy(data);
    let _ = aegis_language::languages::typescript::analyze(source.as_ref());
});
