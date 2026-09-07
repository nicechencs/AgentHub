use super::*;

#[test]
fn pkce_shapes() {
    let p = PkcePair::generate().unwrap();
    assert!(p.verifier().len() >= 32);
    assert!(!p.challenge().is_empty());
    assert_ne!(p.verifier(), p.challenge());
}
