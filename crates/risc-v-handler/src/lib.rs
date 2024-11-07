#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use reth_revm::{handler::register::EvmHandler, Database};

pub fn risc_v_handle_register<EXT, DB: Database>(handler: &mut EvmHandler<'_, EXT, DB>) {}
