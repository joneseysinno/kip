#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let reg = kip::RegistryBuilder::from_seed().freeze();
        let _ = kip::parse_checked(s, &reg, &kip::EmptyResolver);
    }
});
