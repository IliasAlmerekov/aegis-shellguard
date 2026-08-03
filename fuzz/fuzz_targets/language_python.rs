#![no_main]

use libfuzzer_sys::fuzz_target;

// Exercises the public Python adapter on arbitrary UTF-8-lossy source. The
// adapter must remain panic-free for malformed syntax and arbitrary captures.
fuzz_target!(|data: &[u8]| {
    let source = String::from_utf8_lossy(data);
    let _ = aegis_language::languages::python::analyze(source.as_ref());
});
