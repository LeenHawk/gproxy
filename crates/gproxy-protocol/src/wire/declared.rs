//! Same-type rebuilding, not a content IR or a serde round trip.

use std::{
    collections::{BTreeMap, HashMap},
    hash::{BuildHasher, Hash},
    marker::PhantomData,
};

/// Recursively retain declared fields while replacing explicit extension bags
/// with their default value. No extension value is read or cloned. A formal
/// JSON Schema/arguments/metadata field is data, even when it uses the same map
/// or JSON value type as an extension bag, and is therefore retained intact.
pub trait DeclaredFields: Sized {
    fn into_declared(self) -> Self;
}

macro_rules! scalar {
    ($($ty:ty),* $(,)?) => { $(impl DeclaredFields for $ty { fn into_declared(self) -> Self { self } })* };
}
scalar!(
    (),
    bool,
    char,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64,
    String,
    bytes::Bytes,
    serde_json::Number,
    serde_json::Value,
    crate::connection::HttpBody,
    crate::connection::Multipart,
    crate::connection::MultipartPart,
    http::HeaderMap,
    http::Method,
    http::StatusCode
);

impl<T> DeclaredFields for PhantomData<T> {
    fn into_declared(self) -> Self {
        self
    }
}
impl DeclaredFields for &str {
    fn into_declared(self) -> Self {
        self
    }
}
impl<T: DeclaredFields> DeclaredFields for Option<T> {
    fn into_declared(self) -> Self {
        self.map(DeclaredFields::into_declared)
    }
}
impl<T: DeclaredFields> DeclaredFields for Vec<T> {
    fn into_declared(self) -> Self {
        self.into_iter()
            .map(DeclaredFields::into_declared)
            .collect()
    }
}
impl<T: DeclaredFields> DeclaredFields for Box<T> {
    fn into_declared(self) -> Self {
        Box::new((*self).into_declared())
    }
}
impl<T: DeclaredFields, const N: usize> DeclaredFields for [T; N] {
    fn into_declared(self) -> Self {
        self.map(DeclaredFields::into_declared)
    }
}
impl<K: Ord, V: DeclaredFields> DeclaredFields for BTreeMap<K, V> {
    fn into_declared(self) -> Self {
        self.into_iter()
            .map(|(key, value)| (key, value.into_declared()))
            .collect()
    }
}
impl<K: Eq + Hash, V: DeclaredFields, S: BuildHasher> DeclaredFields for HashMap<K, V, S> {
    fn into_declared(mut self) -> Self {
        // Keep the original hasher, including its state, without requiring
        // either it or the values to implement Clone or Default.
        let entries: Vec<_> = self.drain().collect();
        self.extend(
            entries
                .into_iter()
                .map(|(key, value)| (key, value.into_declared())),
        );
        self
    }
}
impl DeclaredFields for serde_json::Map<String, serde_json::Value> {
    fn into_declared(self) -> Self {
        self
    }
}
impl<B: DeclaredFields> DeclaredFields for crate::WireRequest<B> {
    fn into_declared(self) -> Self {
        Self {
            method: self.method,
            path: self.path,
            query: self.query,
            headers: self.headers,
            body: self.body.into_declared(),
        }
    }
}
impl<B: DeclaredFields> DeclaredFields for crate::WireResponse<B> {
    fn into_declared(self) -> Self {
        Self {
            status: self.status,
            headers: self.headers,
            body: self.body.into_declared(),
        }
    }
}
