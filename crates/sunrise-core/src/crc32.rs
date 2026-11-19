//! CRC-32/ISO-HDLC, the checksum used by Java's `CRC32` and ZIP.

#[cfg(target_arch = "x86_64")]
#[allow(unsafe_code)]
mod x86;

/// A streaming ISO-HDLC checksum with runtime feature detection.
pub struct Crc32(Backend);

enum Backend {
    #[cfg(target_arch = "aarch64")]
    Fast(crc_fast::Digest),
    #[cfg(target_arch = "x86_64")]
    Wide(x86::Hasher),
    Standard(crc32fast::Hasher),
}

impl Crc32 {
    pub fn new() -> Self {
        #[cfg(target_arch = "aarch64")]
        if accelerated() {
            return Self(Backend::Fast(crc_fast::Digest::new(
                crc_fast::CrcAlgorithm::Crc32IsoHdlc,
            )));
        }
        #[cfg(target_arch = "x86_64")]
        if let Some(digest) = x86::Hasher::new() {
            return Self(Backend::Wide(digest));
        }
        Self(Backend::Standard(crc32fast::Hasher::new()))
    }

    pub fn checksum(bytes: &[u8]) -> u32 {
        #[cfg(target_arch = "aarch64")]
        if accelerated() {
            return crc_fast::crc32_iso_hdlc(bytes);
        }
        #[cfg(target_arch = "x86_64")]
        if let Some(mut digest) = x86::Hasher::new() {
            digest.update(bytes);
            return digest.finalize();
        }
        crc32fast::hash(bytes)
    }

    pub fn update(&mut self, bytes: &[u8]) {
        match &mut self.0 {
            #[cfg(target_arch = "aarch64")]
            Backend::Fast(digest) => digest.update(bytes),
            #[cfg(target_arch = "x86_64")]
            Backend::Wide(digest) => digest.update(bytes),
            Backend::Standard(digest) => digest.update(bytes),
        }
    }

    pub fn finalize(self) -> u32 {
        match self.0 {
            #[cfg(target_arch = "aarch64")]
            Backend::Fast(digest) => digest.finalize() as u32,
            #[cfg(target_arch = "x86_64")]
            Backend::Wide(digest) => digest.finalize(),
            Backend::Standard(digest) => digest.finalize(),
        }
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_arch = "aarch64")]
fn accelerated() -> bool {
    static USE_FAST: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *USE_FAST.get_or_init(|| {
        // crc-fast 1.10.0 selects its AES tiers without testing CRC.
        std::arch::is_aarch64_feature_detected!("aes")
            && std::arch::is_aarch64_feature_detected!("crc")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_and_short_updates_compose_like_standard_crc32() {
        let mut bytes = [0; 8193];
        let mut state = 1u32;
        for byte in &mut bytes {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *byte = state as u8;
        }
        let bytes = &bytes[1..];
        let expected = crc32fast::hash(bytes);
        assert_eq!(Crc32::checksum(bytes), expected);
        let mut streamed = Crc32::new();
        let (prefix, rest) = bytes.split_at(19);
        let (body, tail) = rest.split_at(4097);
        for chunk in [prefix, body, tail] {
            streamed.update(chunk);
        }
        assert_eq!(streamed.finalize(), expected);
    }

    #[test]
    fn streamed_crc32_matches_the_iso_hdlc_check_value() {
        #[cfg(target_arch = "aarch64")]
        let cases = [
            Crc32::new(),
            Crc32(Backend::Standard(crc32fast::Hasher::new())),
        ];
        #[cfg(not(target_arch = "aarch64"))]
        let cases = [Crc32::new()];
        for mut checksum in cases {
            checksum.update(b"1234");
            checksum.update(b"");
            checksum.update(b"56789");
            assert_eq!(checksum.finalize(), 0xcbf4_3926);
        }
    }
}
