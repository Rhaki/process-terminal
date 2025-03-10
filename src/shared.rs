use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// A wrapper over the `Arc<RwLock<T>>` smart pointer, providing some convenience
/// methods.
#[derive(Debug, Default)]
pub struct Shared<S> {
    inner: Arc<RwLock<S>>,
}

impl<S> Shared<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    pub fn read_access(&self) -> RwLockReadGuard<S> {
        self.inner
            .read()
            .unwrap_or_else(|err| panic!("poisoned lock: {err:?}"))
    }

    pub fn write_access(&self) -> RwLockWriteGuard<S> {
        self.inner
            .write()
            .unwrap_or_else(|err| panic!("poisoned lock: {err:?}"))
    }

    pub fn read_with<F, T>(&self, action: F) -> T
    where
        F: FnOnce(RwLockReadGuard<S>) -> T,
    {
        action(self.read_access())
    }

    pub fn write_with<F, T>(&self, action: F) -> T
    where
        F: FnOnce(RwLockWriteGuard<S>) -> T,
    {
        action(self.write_access())
    }
}

impl<S> Clone for Shared<S> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
