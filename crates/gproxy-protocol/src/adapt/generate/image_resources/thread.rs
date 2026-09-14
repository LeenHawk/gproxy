//! Native resource futures preserve the capability layer's Send contract;
//! browser capabilities may retain thread-local handles.
#[cfg(not(target_arch = "wasm32"))]
pub trait ResourceSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + ?Sized> ResourceSend for T {}
#[cfg(target_arch = "wasm32")]
pub trait ResourceSend {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> ResourceSend for T {}
#[cfg(not(target_arch = "wasm32"))]
pub trait ResourceSync: Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Sync + ?Sized> ResourceSync for T {}
#[cfg(target_arch = "wasm32")]
pub trait ResourceSync {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> ResourceSync for T {}
