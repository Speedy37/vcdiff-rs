use std::io::{Read, Seek, Write};
use vcdiff::{header, window_header, WindowHeader, VCD_SOURCE};
use code_table::{CodeTable, Instruction, InstructionType};
use address_cache::AddressCache;
use nom::{IResult, Needed};
use std::ops::Range;
use std::io;
use varint::VarIntDecode;

#[derive(Debug, PartialEq)]
pub enum DecoderState {
    WantMoreInput,
    WantMoreInputOrDone,
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum DecoderInternalState {
    WantHeader,
    WantWindowHeader,
    WantWindowData,
}

pub struct VCDiffDecoder<ORIGINAL: Read + Seek, TARGET: Write + Read + Seek> {
    original: ORIGINAL,
    target: TARGET,
    state: DecoderInternalState,
    code_table: CodeTable,
    window_header: WindowHeader,
    address_cache: AddressCache,
    buffer: Vec<u8>,
}

impl<ORIGINAL: Read + Seek, TARGET: Write + Read + Seek> VCDiffDecoder<ORIGINAL, TARGET> {
    pub fn new(
        original: ORIGINAL,
        target: TARGET,
        buffer_size: usize,
    ) -> VCDiffDecoder<ORIGINAL, TARGET> {
        VCDiffDecoder {
            original,
            target,
            state: DecoderInternalState::WantHeader,
            code_table: CodeTable::default(),
            window_header: WindowHeader {
                win_indicator: 0,
                source_segment: None,
                delta_encoding_size: 0,
                target_window_size: 0,
                delta_indicator: 0,
                adds_runs_size: 0,
                intructions_size: 0,
                copy_addresses_size: 0,
            },
            buffer: Vec::with_capacity(buffer_size),
            address_cache: AddressCache::new(4, 3),
        }
    }

    fn decode_step<'a>(
        &mut self,
        input: &'a [u8],
    ) -> Result<IResult<&'a [u8], DecoderInternalState>, io::Error> {
        Ok(match self.state {
            DecoderInternalState::WantHeader => match header(input) {
                IResult::Done(remaining, header) => {
                    if let Some(custom_code_table) = header.custom_code_table {
                        self.code_table = custom_code_table;
                    }
                    IResult::Done(remaining, DecoderInternalState::WantWindowHeader)
                }
                IResult::Incomplete(n) => IResult::Incomplete(n),
                IResult::Error(n) => IResult::Error(n),
            },
            DecoderInternalState::WantWindowHeader => match window_header(input) {
                IResult::Done(remaining, window_header) => {
                    self.window_header = window_header;
                    IResult::Done(remaining, DecoderInternalState::WantWindowData)
                }
                IResult::Incomplete(n) => IResult::Incomplete(n),
                IResult::Error(n) => IResult::Error(n),
            },
            DecoderInternalState::WantWindowData => {
                let s1 = self.window_header.adds_runs_size as usize;
                let s2 = s1 + self.window_header.intructions_size as usize;
                let s3 = s2 + self.window_header.copy_addresses_size as usize;
                let want = s3;
                if input.len() < want {
                    IResult::Incomplete(Needed::Size(want))
                } else {
                    self.decode_window(&input[0..s1], &input[s1..s2], &input[s2..s3])?;
                    IResult::Done(&input[s3..], DecoderInternalState::WantWindowHeader)
                }
            }
        })
    }

    fn decode_window(
        &mut self,
        adds_runs: &[u8],
        instructions: &[u8],
        copy_addresses: &[u8],
    ) -> Result<(), io::Error> {
        self.address_cache.reset();
        let mut remaining_adds_runs = adds_runs;
        let mut remaining_addresses = copy_addresses;
        let mut target_data = Vec::with_capacity(self.window_header.target_window_size as usize);
        let address_cache = &mut self.address_cache;
        let window_header = &self.window_header;
        let original = &mut self.original;
        let target = &mut self.target;
        if self.window_header.delta_indicator > 0 {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "compressed delta sections is not supported and won't be",
            ))?;
        }
        {
            let mut decode_inst =
                |inst: Instruction, instructions: &[u8]| -> Result<usize, io::Error> {
                    let mut size = inst.size as usize;
                    let mut remaining_instructions = instructions;
                    if size == 0 {
                        match usize::decode_varint(remaining_instructions) {
                            IResult::Done(r, sz) => {
                                remaining_instructions = r;
                                size = sz;
                            }
                            _ => Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "unable to get instruction size",
                            ))?,
                        };
                    }

                    match inst.typ {
                        InstructionType::Add => {
                            target_data.extend_from_slice(&remaining_adds_runs[0..size]);
                            remaining_adds_runs = &remaining_adds_runs[size..];
                        }
                        InstructionType::Copy => {
                            let source_length = window_header.source_segment.map_or(0u64, |r| r.1);
                            let (r, addr) = address_cache.decode(
                                (target_data.len() as u64) + source_length,
                                inst.mode,
                                remaining_addresses,
                            )?;
                            remaining_addresses = r;

                            let s = window_header.source_segment.and_then(|(pos, sz)| {
                                if addr < sz {
                                    Some(pos)
                                } else {
                                    None
                                }
                            });

                            if let Some(pos) = s {
                                let target_pos = target_data.len();
                                target_data.resize(target_pos + size, 0u8);
                                if (window_header.win_indicator & VCD_SOURCE) > 0 {
                                    original.seek(io::SeekFrom::Start(pos + addr))?;
                                    original.read(&mut target_data[target_pos..target_pos + size])?;
                                } else {
                                    let current = target.seek(io::SeekFrom::Current(0))?;
                                    target.seek(io::SeekFrom::Start(pos + addr))?;
                                    target.read(&mut target_data[target_pos..target_pos + size])?;
                                    target.seek(io::SeekFrom::Start(current))?;
                                }
                            } else {
                                let target_pos = (addr - source_length) as usize;
                                // probably quite slow...
                                for idx in target_pos..target_pos + size {
                                    let byte = target_data[idx];
                                    target_data.push(byte);
                                }
                            }
                        }
                        InstructionType::Run => {
                            let byte = remaining_adds_runs[0];
                            let pos = target_data.len();
                            remaining_adds_runs = &remaining_adds_runs[1..];
                            target_data.resize(pos + size, byte);
                        }
                    };
                    Ok(instructions.len() - remaining_instructions.len())
                };

            let mut remaining_instructions = instructions;
            while let Some((&inst_index, r)) = remaining_instructions.split_first() {
                let e = self.code_table.entries[inst_index as usize];
                remaining_instructions = &r;
                remaining_instructions =
                    &remaining_instructions[decode_inst(e.0, remaining_instructions)?..];
                if let Some(inst) = e.1 {
                    remaining_instructions =
                        &remaining_instructions[decode_inst(inst, remaining_instructions)?..];
                }
            }
        }

        target.write(&target_data)?;

        Ok(())
    }

    pub fn decode(&mut self, input: &[u8]) -> Result<DecoderState, io::Error> {
        use std::mem;

        let mut res: Option<DecoderState> = None;
        let consumed;
        let mut buffer = Vec::new();
        {
            mem::swap(&mut self.buffer, &mut buffer);
        } // little trick to make buffer and self both borrowable

        {
            let available = if buffer.len() == 0 {
                input
            } else {
                buffer.extend_from_slice(&input);
                &buffer
            };
            let mut remaining = available;
            while res.is_none() {
                match self.decode_step(remaining)? {
                    IResult::Done(r, state) => {
                        self.state = state;
                        remaining = r
                    }
                    IResult::Incomplete(_) => {
                        if self.state == DecoderInternalState::WantWindowHeader
                            && remaining.len() == 0
                        {
                            res = Some(DecoderState::WantMoreInputOrDone)
                        } else {
                            res = Some(DecoderState::WantMoreInput)
                        }
                    }
                    IResult::Error(_) => Err(io::Error::new(io::ErrorKind::Other, "format error"))?,
                };
            }

            consumed = available.len() - remaining.len();
        }

        {
            // Ensure remaining data is at the start of the buffer
            if buffer.len() > 0 {
                buffer.drain(Range {
                    start: 0,
                    end: consumed,
                });
            } else {
                buffer.extend_from_slice(&input[consumed..]);
            }
            mem::swap(&mut self.buffer, &mut buffer);
        }

        Ok(res.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::{Read, Seek};
    use nom::IResult;
    use {DecoderState, VCDiffDecoder};

    #[test]
    fn text_1() {
        {
            let mut src = File::open("tst/text-1/src.txt").unwrap();
            let mut patch = File::open("tst/text-1/l.patch").unwrap();
            let mut decoded = File::create("tst/text-1/generated-decoded.txt").unwrap();
            let mut decoder = VCDiffDecoder::new(&mut src, &mut decoded, 128);
            let mut chunk = [0u8; 128];

            let mut state: DecoderState = DecoderState::WantMoreInput;
            let mut read = patch.read(&mut chunk).unwrap();
            while read > 0 {
                state = decoder.decode(&chunk[..read]).unwrap();
                read = patch.read(&mut chunk).unwrap();
            }
            assert_eq!(state, DecoderState::WantMoreInputOrDone);
        }
        {
            let mut target = File::open("tst/text-1/target.txt").unwrap();
            let mut target_data = Vec::new();
            assert!(
                target.read_to_end(&mut target_data).is_ok(),
                "target should be read to end"
            );
            let mut decoded = File::open("tst/text-1/generated-decoded.txt").unwrap();
            let mut decoded_data = Vec::new();
            assert!(
                decoded.read_to_end(&mut decoded_data).is_ok(),
                "decoded should be read to end"
            );
            assert_eq!(decoded_data, target_data);
        }
    }
}
