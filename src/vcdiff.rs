use varint::VarIntDecode;

use nom::{IResult, be_u32};
use code_table::CodeTable;

pub struct VCDiffHeader {
  pub custom_code_table: Option<CodeTable>,
}

#[derive(PartialEq, Debug)]
pub struct WindowHeader {
  /**
    This byte is a set of bits, as shown:

    If bit 0 (VCD_SOURCE) is non-zero, this indicates that a
    segment of data from the "source" file was used as the
    corresponding source window of data to encode the target
    window.  The decoder will use this same source data segment to
    decode the target window.

    If bit 1 (VCD_TARGET) is non-zero, this indicates that a
    segment of data from the "target" file was used as the
    corresponding source window of data to encode the target
    window.  As above, this same source data segment is used to
    decode the target window.

    The Win_Indicator byte MUST NOT have more than one of the bits
    set (non-zero).  It MAY have none of these bits set.

    If one of these bits is set, the byte is followed by two
    integers to indicate respectively, the length and position of
    the source data segment in the relevant file.  If the indicator
    byte is zero, the target window was compressed by itself
    without comparing against another data segment, and these two
    integers are not included.
  */
  pub win_indicator: u8,

  pub source_segment: Option<(u64, u64)>,

  /**
    This integer gives the total number of remaining bytes that
    comprise the data of the delta encoding for this target
    window.
  */
  pub delta_encoding_size: u32,

  /**
    This integer indicates the actual size of the target window
    after decompression.  A decoder can use this value to
    allocate memory to store the uncompressed data.
  */
  pub target_window_size: u32,

  pub delta_indicator: u8,

  /**
    This is the length (in bytes) of the section of data storing
    the unmatched data accompanying the ADD and RUN instructions.
  */
  pub adds_runs_size: u32,

  /**
    This is the length (in bytes) of the delta instructions and
    accompanying sizes.
  */
  pub intructions_size: u32,

  /**
    This is the length (in bytes) of the section storing the
    addresses of the COPY instructions.
  */
  pub copy_addresses_size: u32,
}

static VCD_COMPRESSOR: u8 = 0x01;
static VCD_CODETABLE: u8 = 0x02;
static VCD_APPHEADER: u8 = 0x04;

pub static VCD_SOURCE: u8 = 0x01;
pub static VCD_TARGET: u8 = 0x02;
pub static VCD_ADLER32: u8 = 0x04;

fn u32_decode_varint(i: &[u8]) -> IResult<&[u8], u32> {
  u32::decode_varint(i)
}

fn u64_decode_varint(i: &[u8]) -> IResult<&[u8], u64> {
  u64::decode_varint(i)
}

named!(get_byte<u8>, map!(take!(1), |bs| bs[0]));
named!(header_magic, tag!([0xD6, 0xC3, 0xC4, 0x00]));
named!(app_header<&[u8], ()>, do_parse!(
     sz: u32_decode_varint
  >> take!(sz)
  >>
  ()
));
named!(pub header<VCDiffHeader>, do_parse!(
     header_magic
  >> hdr_indicator: get_byte
  >> custom_code_table: cond!(hdr_indicator & VCD_CODETABLE == VCD_CODETABLE, call!(CodeTable::decode))
  >> cond!(hdr_indicator & VCD_APPHEADER == VCD_APPHEADER, call!(app_header))
  >>
  (VCDiffHeader {
    custom_code_table
  })
));

named!(source_segment_size_pos<&[u8], (u64, u64)>, do_parse!(
     sz: u64_decode_varint
  >> pos: u64_decode_varint
  >>
  (pos, sz)
));
named!(pub window_header<&[u8], WindowHeader>, do_parse!(
     win_indicator: get_byte
  >> source_segment: cond!((win_indicator & (VCD_SOURCE | VCD_TARGET)) > 0, call!(source_segment_size_pos))
  >> delta_encoding_size: u32_decode_varint
  >> target_window_size: u32_decode_varint
  >> delta_indicator: get_byte
  >> adds_runs_size: u32_decode_varint
  >> intructions_size: u32_decode_varint
  >> copy_addresses_size: u32_decode_varint
  >> cond!((win_indicator & VCD_ADLER32) > 0, call!(be_u32))
  >>
  (WindowHeader {
    win_indicator,
    source_segment,
    delta_encoding_size,
    target_window_size,
    delta_indicator,
    adds_runs_size,
    intructions_size,
    copy_addresses_size,
  })
));

/*

05 win_indicator
bc 41 Source segment size 7745
0a Source segment position 10
c5 64 Length of the delta encoding 8932
81 8f 7d target_window_size: 18429
00 Delta_Indicator
97 04
93 72
9a 60
b3 79 97 b0

*/
