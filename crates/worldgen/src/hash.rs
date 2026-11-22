fn mix64(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);

    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);

    x ^ (x >> 31)
}

pub(crate) fn hash_cell(seed: u64, x: i64, y: i64) -> u64 {
    let x = mix64(x as u64);
    let y = mix64(y as u64);

    mix64(seed ^ x ^ y.rotate_left(32))
}

/// Stable 64-bit hash of a name (FNV-1a, then mixed), independent of platform and Rust version.
pub(crate) fn hash_name(name: &str) -> u64 {
    let fnv = name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    mix64(fnv)
}
