use std::io;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::decompress::{self, UnpackedSize};
use crate::error;

#[derive(Debug, Copy, Clone)]
/// LZMA "lclppb" decompression properties.
pub struct LzmaProperties {
    /// The number of literal context bits.
    ///
    /// The most `lc` significant bits of the previous byte are part of the
    /// literal context. `lc` must not be greater than 8.
    pub lc: u32, // 0..=8
    /// The number of literal position bits.
    ///
    /// `lp` must not be greater than 4.
    pub lp: u32, // 0..=4
    /// The number of position bits.
    ///
    /// The context for literal/match is plaintext offset modulo `2^pb`.
    /// `pb` must not be greater than 4.
    pub pb: u32, // 0..=4
}

impl LzmaProperties {
    /// Assert the validity of the LZMA properties.
    pub(crate) fn validate(&self) {
        assert!(self.lc <= 8);
        assert!(self.lp <= 4);
        assert!(self.pb <= 4);
    }
}

#[derive(Debug, Copy, Clone)]
/// LZMA decompression parameters.
pub struct LzmaParams {
    /// The LZMA "lclppb" decompression properties.
    pub properties: LzmaProperties,
    /// The dictionary size to use when decompressing.
    pub dict_size: u32,
    /// The size of the unpacked data.
    pub unpacked_size: Option<u64>,
}

impl LzmaParams {
    /// Create an new instance of LZMA parameters.
    #[cfg(feature = "raw")]
    pub fn new(
        properties: LzmaProperties,
        dict_size: u32,
        unpacked_size: Option<u64>,
    ) -> LzmaParams {
        Self {
            properties,
            dict_size,
            unpacked_size,
        }
    }

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
                    _ => {
                        lzma_info!("Unpacked size: {}", size);
                    }
                }
                stream.write_u64::<LittleEndian>(size)?;
            }
            None => {}
        };

        Ok(())
    }

    /// Read LZMA parameters from the LZMA stream header.
    pub fn read_header<R>(input: &mut R, options: &decompress::Options) -> error::Result<LzmaParams>
    where
        R: io::BufRead,
    {
        // Properties
        let props = input.read_u8().map_err(error::Error::HeaderTooShort)?;

        let mut pb = props as u32;
        if pb >= 225 {
            return Err(error::Error::LzmaError(format!(
                "LZMA header invalid properties: {} must be < 225",
                pb
            )));
        }

        let lc: u32 = pb % 9;
        pb /= 9;
        let lp: u32 = pb % 5;
        pb /= 5;

        lzma_info!("Properties {{ lc: {}, lp: {}, pb: {} }}", lc, lp, pb);

        // Dictionary
        let dict_size_provided = input
            .read_u32::<LittleEndian>()
            .map_err(error::Error::HeaderTooShort)?;
        let dict_size = if dict_size_provided < 0x1000 {
            0x1000
        } else {
            dict_size_provided
        };

        lzma_info!("Dict size: {}", dict_size);

        // Unpacked size
        let unpacked_size: Option<u64> = match options.unpacked_size {
            UnpackedSize::ReadFromHeader => {
                let unpacked_size_provided = input
                    .read_u64::<LittleEndian>()
                    .map_err(error::Error::HeaderTooShort)?;
                let marker_mandatory: bool = unpacked_size_provided == 0xFFFF_FFFF_FFFF_FFFF;
                if marker_mandatory {
                    None
                } else {
                    Some(unpacked_size_provided)
                }
            }
            UnpackedSize::ReadHeaderButUseProvided(x) => {
                input
                    .read_u64::<LittleEndian>()
                    .map_err(error::Error::HeaderTooShort)?;
                x
            }
            UnpackedSize::UseProvided(x) => x,
        };

        lzma_info!("Unpacked size: {:?}", unpacked_size);

        let params = LzmaParams {
            properties: LzmaProperties { lc, lp, pb },
            dict_size,
            unpacked_size,
        };

        Ok(params)
    }
}
