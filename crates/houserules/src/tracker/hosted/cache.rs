//! The read of the issues that a hosted adapter keeps for its life.

use std::cell::RefCell;
use std::rc::Rc;

use crate::tracker::TrackerError;

/// The issues of one hosted adapter: read once, shared by every operation, and
/// dropped by a write.
pub(in crate::tracker) struct IssueCache<T> {
    read: RefCell<Option<Rc<Vec<T>>>>,
}

impl<T> IssueCache<T> {
    /// An empty cache: the first operation that needs the issues reads them.
    pub(in crate::tracker) fn new() -> Self {
        Self {
            read: RefCell::new(None),
        }
    }

    /// The issues that the cache holds, else the ones that `read` returns,
    /// which the cache then holds. A `read` that fails leaves the cache empty.
    pub(in crate::tracker) fn get_or_read(
        &self,
        read: impl FnOnce() -> Result<Vec<T>, TrackerError>,
    ) -> Result<Rc<Vec<T>>, TrackerError> {
        if let Some(held) = self.read.borrow().as_ref() {
            return Ok(Rc::clone(held));
        }
        let issues = Rc::new(read()?);
        *self.read.borrow_mut() = Some(Rc::clone(&issues));
        Ok(issues)
    }

    /// Drops the issues, so that the next operation reads them again. A write
    /// calls it whatever the outcome: a call that failed may have changed the
    /// backend.
    pub(in crate::tracker) fn forget(&self) {
        self.read.borrow_mut().take();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    /// The first call reads, later calls share that read, a failed read leaves
    /// the cache empty, and `forget` makes the next call read again.
    #[test]
    fn the_cache_reads_once_until_it_forgets() {
        let cache = IssueCache::new();
        let reads = Cell::new(0);
        let read = |issues: Vec<u32>| {
            reads.set(reads.get() + 1);
            Ok::<_, TrackerError>(issues)
        };
        assert_eq!(*cache.get_or_read(|| read(vec![1])).expect("read"), [1]);
        assert_eq!(*cache.get_or_read(|| read(vec![2])).expect("held"), [1]);
        assert_eq!(reads.get(), 1);
        cache.forget();
        assert_eq!(*cache.get_or_read(|| read(vec![3])).expect("reread"), [3]);
        cache.forget();
        assert_eq!(
            cache
                .get_or_read(|| Err(TrackerError::Command("boom".to_string())))
                .err(),
            Some(TrackerError::Command("boom".to_string()))
        );
        assert_eq!(*cache.get_or_read(|| read(vec![4])).expect("after"), [4]);
    }
}
