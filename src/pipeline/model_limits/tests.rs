use super::intersect_bool;

#[test]
fn route_thinking_uses_strict_three_state_intersection() {
    assert_eq!(intersect_bool(Some(true), Some(true)), Some(true));
    assert_eq!(intersect_bool(Some(true), None), None);
    assert_eq!(intersect_bool(None, None), None);
    assert_eq!(intersect_bool(Some(false), None), Some(false));
    assert_eq!(intersect_bool(Some(true), Some(false)), Some(false));
}
