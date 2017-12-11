extern crate num_traits;

#[macro_use]
extern crate nom;

mod varint;
mod address_cache;
mod code_table;
mod vcdiff;
mod decoder;

pub use decoder::{VCDiffDecoder, DecoderState};

