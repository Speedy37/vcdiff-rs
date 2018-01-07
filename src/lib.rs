#[macro_use]
extern crate nom;

mod varint;
mod vcdiff;
mod address_cache;
mod code_table;
mod decoder;

mod rolling_hash;
mod encoder;

pub use decoder::{DecoderState, VCDiffDecoder};
