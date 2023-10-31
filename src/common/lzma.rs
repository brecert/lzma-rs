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
    pub(crate) properties: LzmaProperties,
    /// The dictionary size to use when decompressing.
    pub(crate) dict_size: u32,
    /// The size of the unpacked data.
    pub(crate) unpacked_size: Option<u64>,
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
}