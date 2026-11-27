fn fnv1a(hash: &mut u64, text: &str) {
    for byte in text.bytes() {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

pub(crate) fn corpus_hash(
    units: &[&str],
    max_units: usize,
    observe: impl Fn(&str) -> String,
) -> u64 {
    fn walk(
        units: &[&str],
        left: usize,
        input: &mut String,
        hash: &mut u64,
        observe: &impl Fn(&str) -> String,
    ) {
        fnv1a(hash, input);
        fnv1a(hash, "\u{1}");
        fnv1a(hash, &observe(input));
        fnv1a(hash, "\u{2}");
        if left > 0 {
            for unit in units {
                let len = input.len();
                input.push_str(unit);
                walk(units, left - 1, input, hash, observe);
                input.truncate(len);
            }
        }
    }

    let mut hash = 0xcbf2_9ce4_8422_2325;
    walk(units, max_units, &mut String::new(), &mut hash, &observe);
    hash
}
