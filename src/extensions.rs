//! Type-safe request-scoped storage bag indexed by type identifiers.

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt,
};

/// A type-safe container storing heterogeneous data indexed by [`TypeId`].
#[derive(Default)]
pub struct Extensions {
    map: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Extensions {
    /// Creates a new empty `Extensions` container.
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Inserts a typed value into the container, returning the previous value if one existed.
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        self.map
            .insert(TypeId::of::<T>(), Box::new(value))
            .and_then(|boxed| boxed.downcast::<T>().ok().map(|unboxed| *unboxed))
    }

    /// Returns an immutable reference to the value of type `T` if stored.
    #[must_use]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_ref::<T>())
    }

    /// Returns a mutable reference to the value of type `T` if stored.
    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.map
            .get_mut(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_mut::<T>())
    }

    /// Removes and returns the value of type `T` from the container.
    pub fn remove<T: Send + Sync + 'static>(&mut self) -> Option<T> {
        self.map
            .remove(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast::<T>().ok().map(|unboxed| *unboxed))
    }

    /// Checks if a value of type `T` is present in the container.
    #[must_use]
    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }

    /// Returns the number of stored types in the container.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Checks whether the container is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Removes all stored items from the container.
    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl fmt::Debug for Extensions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Extensions")
            .field("len", &self.map.len())
            .finish()
    }
}
