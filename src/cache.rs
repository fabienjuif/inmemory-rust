use std::{
    marker::PhantomData,
    time::{Duration, Instant},
};

pub trait Cache<T> {
    fn new(capacity: usize) -> Self;
    fn get(&self, key: &'static str) -> Option<T>;
    fn set(&mut self, key: &'static str, value: T);
    fn evict(&mut self, key: &'static str);
    fn len(&self) -> usize;
}

#[derive(Clone)]
pub struct CacheWithTTLEntry<T> {
    value: T,
    ttl: Instant,
}

pub struct CacheWithTTL<T, C: Cache<CacheWithTTLEntry<T>>> {
    cache: C,
    _dummy: PhantomData<*const T>,
}

impl<T, C: Cache<CacheWithTTLEntry<T>>> CacheWithTTL<T, C> {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: C::new(capacity),
            _dummy: PhantomData {},
        }
    }

    pub fn get(&mut self, key: &'static str) -> Option<T> {
        let Some(entry) = self.cache.get(key) else {
            return None;
        };
        if entry.ttl <= Instant::now() {
            self.cache.evict(key);
            return None;
        }
        Some(entry.value)
    }

    pub fn set(&mut self, key: &'static str, value: T, ttl: Duration) {
        self.cache.set(
            key,
            CacheWithTTLEntry {
                value,
                ttl: Instant::now() + ttl,
            },
        )
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use crate::{cache::CacheWithTTL, sieve::ESieve};

    #[test]
    fn cache_with_ttl() {
        let mut cache = CacheWithTTL::<String, ESieve<_>>::new(2);
        cache.set("key-1", "value-1".to_string(), Duration::from_millis(100));
        cache.set("key-2", "value-2".to_string(), Duration::from_millis(300));
        assert_eq!(cache.get("key-1"), Some("value-1".to_string()));
        assert_eq!(cache.get("key-2"), Some("value-2".to_string()));
        assert_eq!(cache.len(), 2);
        thread::sleep(Duration::from_millis(150));
        assert_eq!(cache.get("key-1"), None);
        assert_eq!(cache.get("key-2"), Some("value-2".to_string()));
        assert_eq!(cache.len(), 1);
    }
}
