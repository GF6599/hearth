use hearth::splitmix;

/// Neighbouring inputs give unrelated outputs.
/// Given: each pair of neighbouring inputs from 0 to 4096.
/// When: both inputs of the pair are mixed.
/// Then: the two outputs differ in about half of their 64 bits on average.
///
/// Why it matters: the scenes feed it neighbouring tiles and places. An
/// output that keeps most bits of its neighbour lines the noise up into
/// a grid.
#[test]
fn neighbouring_inputs_differ_in_about_half_their_bits() {
    const PAIRS: u32 = 4096;

    // When:
    let flipped: u32 = (0..u64::from(PAIRS))
        .map(|value| (splitmix::mix(value) ^ splitmix::mix(value + 1)).count_ones())
        .sum();

    // Then:
    let average = f64::from(flipped) / f64::from(PAIRS);
    assert!(
        (30.0..=34.0).contains(&average),
        "average of {average} bits"
    );
}
