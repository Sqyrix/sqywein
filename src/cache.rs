//! Küçük, sınırlı LRU önbelleği. Kullanılmayan Rc/görseller çıkarılınca bırakılır.
use std::{collections::HashMap, hash::Hash};

pub struct Cache<K, V> {
    entries: HashMap<K, (V, u64)>,
    capacity: usize,
    clock: u64,
}

impl<K: Eq + Hash + Clone, V> Cache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self { entries: HashMap::new(), capacity, clock: 0 }
    }
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let entry = self.entries.get_mut(key)?;
        self.clock = self.clock.saturating_add(1);
        entry.1 = self.clock;
        Some(&entry.0)
    }
    pub fn insert(&mut self, key: K, value: V) {
        if self.capacity == 0 { return; }
        if !self.entries.contains_key(&key) { self.trim_to(self.capacity - 1); }
        self.clock = self.clock.saturating_add(1);
        self.entries.insert(key, (value, self.clock));
    }
    pub fn remove(&mut self, key: &K) {
        self.entries.remove(key);
    }
    pub fn trim_to(&mut self, count: usize) {
        while self.entries.len() > count {
            let oldest = self.entries.iter().min_by_key(|(_, (_, stamp))| stamp).map(|(k, _)| k.clone());
            if let Some(key) = oldest { self.entries.remove(&key); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    #[test]
    fn evicts_unused_and_releases_ownership() {
        let item = Rc::new(42);
        let mut cache = Cache::new(2);
        cache.insert("a", item.clone()); cache.insert("b", Rc::new(7));
        assert_eq!(Rc::strong_count(&item), 2);
        cache.get(&"b"); cache.insert("c", Rc::new(8));
        assert!(cache.get(&"a").is_none());
        assert_eq!(Rc::strong_count(&item), 1);
        assert_eq!(**cache.get(&"b").unwrap(), 7);
        cache.insert("b", Rc::new(9));
        assert_eq!(cache.entries.len(), 2);
        cache.trim_to(0); assert!(cache.entries.is_empty());
    }
}
