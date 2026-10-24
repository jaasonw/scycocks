pub const REPORT_ID: u8 = 8;
pub const REPORT_LEN: usize = 17;

pub fn validate_reply(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() != REPORT_LEN || bytes[0] != REPORT_ID {
        return Err("Unexpected HID report length or report ID.".into());
    }
    if bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)) != 0x55 {
        return Err("The HID reply checksum is invalid.".into());
    }
    Ok(())
}

#[cfg(any(feature = "gui", test))]
pub fn pairing_packet() -> [u8; REPORT_LEN] {
    let mut bytes = packet(5, 0, 3, &[0, 0, 62]).expect("Fixed pairing payload fits a report");
    // The vendor uses a length of two but places the company ID in the third data byte.
    bytes[5] = 2;
    bytes[16] = 0x55u8.wrapping_sub(bytes[..16].iter().fold(0u8, |sum, b| sum.wrapping_add(*b)));
    bytes
}

pub fn supported(vid: u16, pid: u16) -> bool {
    vid == 0x3554 && matches!(pid, 0xf5f4 | 0xf5f7 | 0xf517 | 0xf5f6 | 0xf515)
}

pub fn packet(
    command: u8,
    address: u16,
    length: u8,
    data: &[u8],
) -> Result<[u8; REPORT_LEN], String> {
    if length > 10 || data.len() > usize::from(length) {
        return Err("A report can contain at most 10 data bytes.".into());
    }
    let mut bytes = [0; REPORT_LEN];
    bytes[0] = REPORT_ID;
    bytes[1] = command;
    bytes[3..5].copy_from_slice(&address.to_be_bytes());
    bytes[5] = length;
    bytes[6..6 + data.len()].copy_from_slice(data);
    bytes[16] = 0x55u8.wrapping_sub(bytes[..16].iter().fold(0u8, |sum, b| sum.wrapping_add(*b)));
    Ok(bytes)
}
