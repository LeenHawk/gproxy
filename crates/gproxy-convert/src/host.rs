//! The capabilities a conversion runs over, named as one bundle so a driver
//! takes a single type parameter.

use gproxy_protocol::{
    OperationKey,
    capability::{ResourceAccess, StateStore, Upstream},
};
use std::{future::Future, pin::Pin};

/// `Send` where the target has threads. Protocol's futures and streams are
/// `Send` on native and not on wasm32; the bounds here follow them.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + ?Sized> MaybeSend for T {}
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSend for T {}

#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSync: Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Sync + ?Sized> MaybeSync for T {}
#[cfg(target_arch = "wasm32")]
pub trait MaybeSync {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSync for T {}

/// A background task handed to [`Host::spawn`].
#[cfg(not(target_arch = "wasm32"))]
pub type Task = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
#[cfg(target_arch = "wasm32")]
pub type Task = Pin<Box<dyn Future<Output = ()> + 'static>>;

pub trait Host {
    /// Upstream calls already bound to a provider and a credential; the
    /// target is the native operation to dispatch.
    type Upstream: Upstream<Target = OperationKey> + Clone + MaybeSend + MaybeSync + 'static;
    /// Continuation state. Owned by a driven client stream, hence `Clone`.
    type State: StateStore<Scope = Self::StateScope> + Clone + MaybeSend + MaybeSync + 'static;
    type StateScope: Clone + MaybeSend + MaybeSync + 'static;
    type Resources: ResourceAccess<Scope = Self::ResourceScope> + MaybeSync;
    type ResourceScope: MaybeSync;

    /// Run `task` to completion in the background. Only a generation over a
    /// Responses WebSocket upstream needs it: the task owns the socket.
    fn spawn(task: Task);
}
