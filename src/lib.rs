#[macro_use]
extern crate nom;

mod address_cache;
mod code_table;
mod decoder;
mod varint;
mod vcdiff;

#[cfg(encoder)]
mod encoder;
#[cfg(encoder)]
mod rolling_hash;

pub use decoder::{DecoderState, ReadSlice, VCDiffDecoder};
