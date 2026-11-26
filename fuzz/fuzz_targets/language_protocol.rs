#![no_main]

use libfuzzer_sys::fuzz_target;


fuzz_target!(|data: &[u8]| {
    let _ = aegis_language::protocol::decode_request(data);
    let _ = aegis_language::protocol::decode_response(data);
});