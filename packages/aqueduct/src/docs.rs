//! Aqueduct's docs, baked into Rust modules
//!
//! # Aqueduct documentation
//! 
//! ## Section 1: Using Aqueduct
//! 
//! This section goes into the various features of Aqueduct and how to use them.
//! 
//! - [Ch. 1.1: Channels](ch_1_01_channels)
//! 
//! ## Section 2: The design of Aqueduct
//! 
//! This section explains various design decisions of the Aqueduct library and protocol and their underlying motivations.
//! 
//! - [Ch. 2.1: Aqueduct design principles](ch_2_01_principles)
//! - [Ch. 2.2: Aqueduct's plan to ZeroRTT everything](ch_2_02_zero_rtt)
#![allow(rustdoc::invalid_rust_codeblocks)]
#![cfg(not(doctest))]

#[doc = include_str!("../../../docs/CHANNELS.md")]
pub mod ch_1_01_channels {}

#[doc = include_str!("../../../docs/PRINCIPLES.md")]
pub mod ch_2_01_principles {}
#[doc = include_str!("../../../docs/ZERO_RTT.md")]
pub mod ch_2_02_zero_rtt {}
