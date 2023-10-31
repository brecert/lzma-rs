use crate::compress::{Options, UnpackedSize};
use crate::encode::rangecoder;
use crate::{error, LzmaParams};
use byteorder::{LittleEndian, WriteBytesExt};
use std::io;

impl LzmaParams {
    /// Write LZMA parameters to the LZMA stream header.
    pub fn write_header<W>(&self, stream: &mut W) -> error::Result<()>
    where
        W: io::Write,
    {
        // Properties
        let properties = self.properties;
        let props = (properties.lc + 9 * (properties.lp + 5 * properties.pb)) as u8;
        lzma_info!("{:?}", properties);
        stream.write_u8(props)?;

        // Dictionary
        lzma_info!("Dict size: {}", self.dict_size);
        stream.write_u32::<LittleEndian>(self.dict_size)?;

        // Unpacked size
        // todo: make behavior symetrical with `read_header`
        match self.unpacked_size {
            Some(size) => {
                match size {
                    0xFFFF_FFFF_FFFF_FFFF => {
                        lzma_info!("Unpacked size: unknown");
                    }
                    size => {
                        lzma_info!("Unpacked size: {}", size);
                    }
                }
                stream.write_u64::<LittleEndian>(size)?;
            }
            None => {}
        };

        Ok(())
    }
}

/// Raw encoder for LZMA.
#[derive(Debug)]
pub struct Encoder<'a, W>
where
    W: 'a + io::Write,
{
    rangecoder: rangecoder::RangeEncoder<'a, W>,
    literal_probs: [[u16; 0x300]; 8],
    is_match: [u16; 4], // true = LZ, false = literal
    unpacked_size: UnpackedSize,
}

const LC: u32 = 3;
const LP: u32 = 0;
const PB: u32 = 2;
const DICT_SIZE: u32 = 0x0080_0000;

impl<'a, W> Encoder<'a, W>
where
    W: io::Write,
{
    #[cfg(feature = "raw")]
    /// Create a new raw encoder
    pub fn new(stream: &'a mut W, options: &Options) -> Self {
        Encoder {
            rangecoder: rangecoder::RangeEncoder::new(stream),
            literal_probs: [[0x400; 0x300]; 8],
            is_match: [0x400; 4],
            unpacked_size: options.unpacked_size,
        }
    }

    /// Create a new encoder by reading from a stream.
    /// This includes reading the header.
    pub fn from_stream(stream: &'a mut W, options: &Options) -> io::Result<Self> {
        // Properties
        let props = (LC + 9 * (LP + 5 * PB)) as u8;
        lzma_info!("Properties {{ lc: {}, lp: {}, pb: {} }}", LC, LP, PB);
        stream.write_u8(props)?;

        // Dictionary
        lzma_info!("Dict size: {}", DICT_SIZE);
        stream.write_u32::<LittleEndian>(DICT_SIZE)?;

        // Unpacked size
        match &options.unpacked_size {
            UnpackedSize::WriteToHeader(unpacked_size) => {
                let value: u64 = match unpacked_size {
                    None => {
                        lzma_info!("Unpacked size: unknown");
                        0xFFFF_FFFF_FFFF_FFFF
                    }
                    Some(x) => {
                        lzma_info!("Unpacked size: {}", x);
                        *x
                    }
                };
                stream.write_u64::<LittleEndian>(value)?;
            }
            UnpackedSize::SkipWritingToHeader => {}
        };

        let encoder = Encoder {
            rangecoder: rangecoder::RangeEncoder::new(stream),
            literal_probs: [[0x400; 0x300]; 8],
            is_match: [0x400; 4],
            unpacked_size: options.unpacked_size,
        };

        Ok(encoder)
    }

    /// Process LZMA stream data.
    /// Will iterate through bytes and encode them sequential until finished.
    pub fn process<R>(mut self, input: R) -> io::Result<()>
    where
        R: io::Read,
    {
        let mut prev_byte = 0u8;
        let mut input_len = 0;

        for (out_len, byte_result) in input.bytes().enumerate() {
            let byte = byte_result?;
            let pos_state = out_len & 3;
            input_len = out_len;

            // Literal
            self.rangecoder
                .encode_bit(&mut self.is_match[pos_state], false)?;

            self.encode_literal(byte, prev_byte)?;
            prev_byte = byte;
        }

        self.finish(input_len + 1)
    }

    fn finish(&mut self, input_len: usize) -> io::Result<()> {
        match self.unpacked_size {
            UnpackedSize::SkipWritingToHeader | UnpackedSize::WriteToHeader(Some(_)) => {}
            UnpackedSize::WriteToHeader(None) => {
                // Write end-of-stream marker
                let pos_state = input_len & 3;

                // Match
                self.rangecoder
                    .encode_bit(&mut self.is_match[pos_state], true)?;
                // New distance
                self.rangecoder.encode_bit(&mut 0x400, false)?;

                // Dummy len, as small as possible (len = 0)
                for _ in 0..4 {
                    self.rangecoder.encode_bit(&mut 0x400, false)?;
                }

                // Distance marker = 0xFFFFFFFF
                // pos_slot = 63
                for _ in 0..6 {
                    self.rangecoder.encode_bit(&mut 0x400, true)?;
                }
                // num_direct_bits = 30
                // result = 3 << 30 = C000_0000
                //        + 3FFF_FFF0  (26 bits)
                //        + F          ( 4 bits)
                for _ in 0..30 {
                    self.rangecoder.encode_bit(&mut 0x400, true)?;
                }
                //        = FFFF_FFFF
            }
        }

        // Flush range coder
        self.rangecoder.finish()
    }

    fn encode_literal(&mut self, byte: u8, prev_byte: u8) -> io::Result<()> {
        let prev_byte = prev_byte as usize;

        let mut result: usize = 1;
        let lit_state = prev_byte >> 5;
        let probs = &mut self.literal_probs[lit_state];

        for i in 0..8 {
            let bit = ((byte >> (7 - i)) & 1) != 0;
            self.rangecoder.encode_bit(&mut probs[result], bit)?;
            result = (result << 1) ^ (bit as usize);
        }

        Ok(())
    }
}
