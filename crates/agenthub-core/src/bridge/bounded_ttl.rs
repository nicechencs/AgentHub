//! Cap + idle-TTL map used by sticky routing and continuation bindings.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

pub(crate) struct BoundedTtlMap<K, V> {
    max: usize,
    ttl: Duration,
    next_stamp: u64,
    items: HashMap<K, TtlEntry<V>>,
}

struct TtlEntry<V> {
    value: V,
    last_used: Instant,
    stamp: u64,
}

impl<K, V> BoundedTtlMap<K, V>
where
    K: Eq + Hash + Clone,
{
    pub(crate) fn new(max: usize, ttl: Duration) -> Self {
        Self {
            max: max.max(1),
            ttl,
            next_stamp: 0,
            items: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    pub(crate) fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        if self.is_expired(key) {
            self.items.remove(key);
            return None;
        }
        let stamp = self.alloc_stamp();
        let entry = self.items.get_mut(key)?;
        entry.last_used = Instant::now();
        entry.stamp = stamp;
        Some(&entry.value)
    }

    #[cfg(test)]
    pub(crate) fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        if self.is_expired(key) {
            self.items.remove(key);
            return None;
        }
        let stamp = self.alloc_stamp();
        let entry = self.items.get_mut(key)?;
        entry.last_used = Instant::now();
        entry.stamp = stamp;
        Some(&mut entry.value)
    }

    pub(crate) fn insert(&mut self, key: K, value: V) {
        if self.items.contains_key(&key) {
            let stamp = self.alloc_stamp();
            if let Some(entry) = self.items.get_mut(&key) {
                entry.value = value;
                entry.last_used = Instant::now();
                entry.stamp = stamp;
            }
            return;
        }
        self.evict_for_capacity();
        let stamp = self.alloc_stamp();
        self.items.insert(
            key,
            TtlEntry {
                value,
                last_used: Instant::now(),
                stamp,
            },
        );
    }

    pub(crate) fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.items.remove(key).map(|entry| entry.value)
    }

    pub(crate) fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(&K, &V) -> bool,
    {
        self.items.retain(|key, entry| keep(key, &entry.value));
    }

    #[cfg(test)]
    pub(crate) fn keys(&self) -> Vec<K> {
        self.items.keys().cloned().collect()
    }

    fn is_expired<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.items
            .get(key)
            .is_some_and(|entry| entry.last_used.elapsed() > self.ttl)
    }

    fn alloc_stamp(&mut self) -> u64 {
        let stamp = self.next_stamp;
        self.next_stamp = self.next_stamp.wrapping_add(1);
        stamp
    }

    fn evict_for_capacity(&mut self) {
        if self.items.len() < self.max {
            return;
        }
        let expired = self
            .items
            .iter()
            .find_map(|(key, entry)| (entry.last_used.elapsed() > self.ttl).then(|| key.clone()));
        if let Some(key) = expired {
            self.items.remove(&key);
            if self.items.len() < self.max {
                return;
            }
        }
        let oldest = self
            .items
            .iter()
            .min_by(|(_, left), (_, right)| {
                left.last_used
                    .cmp(&right.last_used)
                    .then(left.stamp.cmp(&right.stamp))
            })
            .map(|(key, _)| key.clone());
        if let Some(key) = oldest {
            self.items.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests;
