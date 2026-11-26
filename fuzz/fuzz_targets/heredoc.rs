#![no_main]

use aegis_parser::extract_heredoc_bodies;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    let bodies = extract_heredoc_bodies(input.as_ref());

    for body in bodies {
        let _ = body.delimiter.len();
        let _ = body.body.len();
        let _ = body.is_nowdoc;
    }
});