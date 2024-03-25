// https://cachemon.github.io/SIEVE-website/blog/2023/12/17/sieve-is-simpler-than-lru/#wed-love-to-hear-from-you

use crate::cache::Cache;
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, RwLock},
};

struct SieveNode {
    key: &'static str, // TODO: in this impl we could remove it
    visited: bool,
}

struct Sieve {
    capacity: usize,
    cache: HashMap<&'static str, Arc<RefCell<SieveNode>>>,
    log: VecDeque<Arc<RefCell<SieveNode>>>,
    hand: Option<usize>,
}

impl Sieve {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            cache: HashMap::with_capacity(capacity),
            log: VecDeque::with_capacity(capacity),
            hand: None,
        }
    }

    fn get_mut_node(&mut self, key: &'static str) -> Option<Arc<RefCell<SieveNode>>> {
        self.cache.get_mut(key).cloned()
    }

    fn remove(&mut self, key: &'static str) {
        let Some(entry) = self.cache.remove_entry(key) else {
            return;
        };

        // FIXME: FUCK MEEEEEEEEEEEEEEE we have to go against the whole array
        // FIXME: maybe a "dead" marker so we do not mark it live until we insert it again :thinking:
        // FIXME: an other solution is to not "touch" sierde if we have reached TTL, so the key will finally leave with LRU
        let key = entry.1.borrow_mut().key;
        let mut index = None;
        for i in 0..self.log.len() {
            if let Some(node) = self.log.get(i) {
                if node.borrow_mut().key == key {
                    index = Some(i);
                    break;
                }
            }
        }
        if let Some(index) = index {
            self.log.remove(index);
        }
    }

    fn insert(&mut self, key: &'static str) {
        // already exists, we mark it as visited and move on
        let node = self.get_mut_node(key);
        if let Some(node) = node {
            let mut node = (*node).borrow_mut();
            node.visited = true;
            return;
        }

        // new node to insert
        let node = SieveNode {
            key,
            visited: false,
        };

        // if we do not hit full capacity we have this shortcut
        if self.log.len() < self.capacity {
            let node = Arc::new(RefCell::new(node));
            self.log.push_front(node.clone());
            self.cache.insert(key, node);
            return;
        }

        // otherwise, we use sieve algorithm and then push
        self.evict();
        let node = Arc::new(RefCell::new(node));
        self.log.push_front(node.clone());
        self.cache.insert(key, node);
    }

    // actual evict algorithm is here
    fn evict(&mut self) {
        let mut hand = self.hand.unwrap_or(self.log.len() - 1);
        loop {
            let Some(obj) = self.log.get(hand) else {
                break;
            };
            let mut obj = (*obj).borrow_mut();
            if !obj.visited {
                break;
            }
            obj.visited = false;
            hand = if hand == 0 {
                self.log.len() - 1
            } else {
                hand - 1
            };
        }
        self.hand = Some(hand);
        if let Some(obj) = self.log.remove(hand) {
            let obj = (*obj).borrow_mut();
            self.cache.remove(obj.key);
        }
    }

    fn get_keys(&self) -> Vec<&'static str> {
        self.log
            .iter()
            .map(|node| {
                let node = (*node).borrow_mut();
                node.key
            })
            .collect::<Vec<_>>()
    }
}

// Sieve (see link above)
// but eventually writes access log (visited marker) and move access log (queue)
pub struct ESieve<T: Clone> {
    cache: RwLock<HashMap<&'static str, T>>,
    sieve: Mutex<Sieve>,
}

// FIXME: do we need this?
// unsafe impl<T: Clone> Send for ESieve<T> {}
// unsafe impl<T: Clone> Sync for ESieve<T> {}

impl<T: Clone> Cache<T> for ESieve<T> {
    fn new(capacity: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::with_capacity(capacity)),
            sieve: Mutex::new(Sieve::new(capacity)),
        }
    }

    fn get(&self, key: &'static str) -> Option<T> {
        // access the value
        let cache = self.cache.read().unwrap();
        let value = cache.get(key);

        // no try (if we have the lock) to update the access log
        // TODO: make a parameter to swap betweem eventual lock or not
        // TODO: add metrics
        if let Ok(mut sieve) = self.sieve.try_lock() {
            sieve.insert(key);
        };

        value.cloned()
    }

    fn set(&mut self, key: &'static str, value: T) {
        let mut cache = self.cache.write().unwrap();
        cache.insert(key, value);

        // TODO: make a parameter to swap betweem eventual lock or not
        // TODO: add metrics
        let mut sieve = self.sieve.lock().unwrap();
        sieve.insert(key);
    }

    fn evict(&mut self, key: &'static str) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(key);

        // TODO: make a parameter to swap betweem eventual lock or not
        // TODO: add metrics
        let mut sieve = self.sieve.lock().unwrap();
        sieve.remove(key);
    }

    fn len(&self) -> usize {
        self.cache.read().map(|cache| cache.len()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use crate::sieve::Sieve;

    #[test]
    fn sieve() {
        let mut sieve = Sieve::new(7);
        // G*FEDCB*A* (prepare)
        sieve.insert("A");
        sieve.insert("B");
        sieve.insert("C");
        sieve.insert("D");
        sieve.insert("E");
        sieve.insert("F");
        sieve.insert("G");
        assert_eq!(sieve.get_keys(), ["G", "F", "E", "D", "C", "B", "A"]);
        sieve.get_mut_node("A").unwrap().borrow_mut().visited = true;
        sieve.get_mut_node("B").unwrap().borrow_mut().visited = true;
        sieve.get_mut_node("C").unwrap().borrow_mut().visited = false;
        sieve.get_mut_node("D").unwrap().borrow_mut().visited = false;
        sieve.get_mut_node("E").unwrap().borrow_mut().visited = false;
        sieve.get_mut_node("F").unwrap().borrow_mut().visited = false;
        sieve.get_mut_node("G").unwrap().borrow_mut().visited = true;
        sieve.hand = Some(6);

        // HADIBJ (actual test)
        let items = Vec::from(["H", "A", "D", "I", "B", "J"]);
        for item in items {
            sieve.insert(item);
        }

        // simplifield log
        // 1. keys
        assert_eq!(sieve.get_keys(), ["J", "I", "H", "G", "D", "B", "A"]);
        // 2. visited
        let log = sieve
            .log
            .iter()
            .map(|node| node.clone().borrow_mut().visited)
            .collect::<Vec<_>>();
        let log = log.as_slice();
        assert_eq!(log, [false, false, false, true, false, true, true]);
    }
}
