use std::time::Duration;

use super::BoundedTtlMap;

#[test]
fn insert_evicts_oldest_when_over_cap() {
    let mut map = BoundedTtlMap::new(2, Duration::from_secs(60));
    map.insert("a".to_owned(), 1);
    map.insert("b".to_owned(), 2);
    map.insert("c".to_owned(), 3);
    assert_eq!(map.len(), 2);
    let keys = map.keys();
    assert!(keys.contains(&"c".to_owned()));
    assert!(!keys.contains(&"a".to_owned()));
    assert!(map.get("c").is_some());
}

#[test]
fn get_touches_and_protects_from_cap_eviction() {
    let mut map = BoundedTtlMap::new(2, Duration::from_secs(60));
    map.insert("a".to_owned(), 1);
    map.insert("b".to_owned(), 2);
    assert_eq!(map.get("a").copied(), Some(1));
    map.insert("c".to_owned(), 3);
    assert_eq!(map.get("a").copied(), Some(1));
    assert!(map.get("b").is_none());
    assert_eq!(map.get("c").copied(), Some(3));
}

#[test]
fn get_drops_expired_entries() {
    let mut map = BoundedTtlMap::new(8, Duration::from_millis(1));
    map.insert("a".to_owned(), 1);
    std::thread::sleep(Duration::from_millis(5));
    assert!(map.get("a").is_none());
    assert_eq!(map.len(), 0);
}

#[test]
fn retain_and_remove_keep_member_drop_behavior() {
    let mut map = BoundedTtlMap::new(8, Duration::from_secs(60));
    map.insert("one".to_owned(), "acc-a".to_owned());
    map.insert("two".to_owned(), "acc-b".to_owned());
    map.remove("one");
    assert!(map.get("one").is_none());
    map.retain(|_, member| member != "acc-b");
    assert_eq!(map.len(), 0);
}
