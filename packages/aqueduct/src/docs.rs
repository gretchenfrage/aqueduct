//! This module contains aqueduct's docs in the form of Rust module docs, as a trick to get our
//! long-form markdown documents included in the API docs and hosted on docs.rs.
#![allow(rustdoc::invalid_rust_codeblocks)]
#![cfg(not(doctest))]

#[doc = include_str!("../../../docs/PRINCIPLES.md")]
pub mod ch_01_principles {}
#[doc = include_str!("../../../docs/ZERO_RTT.md")]
pub mod ch_02_zero_rtt {}
