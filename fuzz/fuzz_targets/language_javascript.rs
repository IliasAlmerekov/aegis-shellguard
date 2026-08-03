#![no_main]

use libfuzzer_sys::fuzz_target;

// Exercises the public JavaScript adapter on malformed and adversarial source.
fuzz_target!(|data: &[u8]| {
    let source = String::from_utf8_lossy(data);
    let _ = aegis_language::languages::javascript::analyze(source.as_ref());
});
