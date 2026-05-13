#![no_main]

use graph::fuzz_support::FilterIndex;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };

    let mut index = FilterIndex::new();
    index.register_column(1, "amount".to_string(), 1);
    index.register_column(1, "risk".to_string(), 1);
    index.register_column(1, "金额".to_string(), 1);

    let _ = index.parse_condition(input);
});
