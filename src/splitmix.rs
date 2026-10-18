//! The splitmix64 finaliser that the scenes share.

/// Scatters `value` across all 64 bits, so neighbouring inputs give
/// unrelated outputs. It gives noise that depends only on a place, so the
/// noise is the same every frame. A seeded generator cannot do this.
pub fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
