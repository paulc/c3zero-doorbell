// We can only use 15 byte string as NVS key
pub fn hash_ssid(ssid: &str) -> heapless::String<15> {
    const FNV_OFFSET: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    // FNV1 64bit hash
    let mut hash = FNV_OFFSET;
    ssid.as_bytes().iter().for_each(|b| {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    });

    // Convert to hex - only use bottom 60bits
    let mut buffer: heapless::Vec<u8, 15> = heapless::Vec::new();
    for _ in 0..15 {
        let nibble = (hash & 0xF) as usize;
        // Buffer is 16 bytes long so dont need to check
        unsafe { buffer.push_unchecked(HEX_CHARS[nibble]) };
        hash >>= 4;
    }
    // We know this is valid UTF8
    heapless::String::from_utf8(buffer).unwrap()
}
