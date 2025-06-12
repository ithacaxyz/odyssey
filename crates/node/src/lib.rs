//! Standalone crate for Odyssey's node configuration and builder types.
//!
//! This contains mainly the [`OdysseyNode`](node::OdysseyNode) type.
//!
//! The [`OdysseyNode`](node::OdysseyNode) type implements the
//! [`NodeTypes`](reth_op::node::builder::NodeTypes) trait, and configures the engine types required for
//! the optimism engine API.

#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![warn(unused_crate_dependencies)]

pub mod broadcaster;
pub mod chainspec;
pub mod delayed_resolve;
pub mod forwarder;
pub mod node;
pub mod rpc;
