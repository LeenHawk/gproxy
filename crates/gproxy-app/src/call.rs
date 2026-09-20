//! The data plane bridge: `DataPlaneRequest` → `Admitted` → the sdk's
//! `call()`, with the allowed provider and credential sets, the budget owner
//! chain, the scope and the session identity attached. Lands in P4, once
//! `gproxy-sdk` exists.
