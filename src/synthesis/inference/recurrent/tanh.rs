pub(super) fn apply(input: &[f32], output: &mut [f32]) {
    assert_eq!(input.len(), output.len());
    for (&value, target) in input.iter().zip(output.iter_mut()) {
        *target = value.tanh();
    }
}
