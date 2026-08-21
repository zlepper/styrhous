use super::*;

#[test]
fn detail_column_weight_is_always_positive_and_finite() {
    assert_eq!(DetailColumn::new("Label").weight(0.0).weight, 0.1);
    assert_eq!(DetailColumn::new("Label").weight(f32::NAN).weight, 1.0);
    assert_eq!(DetailColumn::new("Label").weight(f32::INFINITY).weight, 1.0);
}
