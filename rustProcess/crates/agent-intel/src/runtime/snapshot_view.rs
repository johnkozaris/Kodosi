use std::sync::Arc;

use arc_swap::ArcSwap;

#[derive(Debug)]
pub struct SnapshotView<T> {
    inner: ArcSwap<T>,
}

impl<T> SnapshotView<T> {
    pub fn new(initial: T) -> Self {
        Self {
            inner: ArcSwap::from_pointee(initial),
        }
    }

    pub fn load(&self) -> Arc<T> {
        self.inner.load_full()
    }

    pub fn store(&self, next: T) {
        self.inner.store(Arc::new(next));
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests panic for failure clarity"
)]
mod tests {
    use super::SnapshotView;

    #[test]
    fn load_returns_initial_value() {
        let view: SnapshotView<u32> = SnapshotView::new(0);
        assert_eq!(*view.load(), 0);
    }

    #[test]
    fn store_replaces_value() {
        let view: SnapshotView<u32> = SnapshotView::new(0);
        view.store(42);
        assert_eq!(*view.load(), 42);
    }
}
