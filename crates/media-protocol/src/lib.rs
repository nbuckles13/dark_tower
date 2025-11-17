//! Proprietary media protocol for Dark Tower.
//!
//! This crate implements a custom binary protocol for transporting
//! audio and video frames over QUIC with minimal overhead.

#![warn(clippy::pedantic)]

pub mod frame;
pub mod codec;
pub mod stream;
