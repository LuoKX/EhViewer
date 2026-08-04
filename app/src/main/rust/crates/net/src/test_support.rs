use std::fmt::Write as _;

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("writing to String should not fail");
    }
    output
}
