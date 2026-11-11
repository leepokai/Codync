//! Source: Hermes tools/audio_container.py at 10d25f8a (MIT).
//! Adaptation: typed containers and MIME names for the host's existing speech providers.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Container {
    M4a,
    Mp4,
    Ogg,
    Flac,
    Wav,
    Mp3,
    Aac,
    Webm,
}

impl Container {
    pub(super) fn sniff(data: &[u8]) -> Option<Self> {
        if data.len() >= 8 && &data[4..8] == b"ftyp" {
            return Some(
                if data.len() >= 12
                    && (data[8..12].eq_ignore_ascii_case(b"m4a ") || data[8..12].eq_ignore_ascii_case(b"m4b "))
                {
                    Self::M4a
                } else {
                    Self::Mp4
                },
            );
        }
        for (prefix, kind) in
            [(b"OggS".as_slice(), Self::Ogg), (b"fLaC".as_slice(), Self::Flac), (b"ID3".as_slice(), Self::Mp3)]
        {
            if data.starts_with(prefix) {
                return Some(kind);
            }
        }
        if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WAVE" {
            return Some(Self::Wav);
        }
        if data.len() >= 2 && data[0] == 0xff && data[1] & 0xe0 == 0xe0 {
            return Some(if data[1] & 0xf6 == 0xf0 { Self::Aac } else { Self::Mp3 });
        }
        data.starts_with(b"\x1a\x45\xdf\xa3").then_some(Self::Webm)
    }

    pub(super) fn extension(self) -> &'static str {
        match self {
            Self::M4a | Self::Mp4 => "m4a",
            Self::Ogg => "ogg",
            Self::Flac => "flac",
            Self::Wav => "wav",
            Self::Mp3 => "mp3",
            Self::Aac => "aac",
            Self::Webm => "webm",
        }
    }

    pub(super) fn mime(self) -> &'static str {
        match self {
            Self::M4a | Self::Mp4 => "audio/mp4",
            Self::Ogg => "audio/ogg",
            Self::Flac => "audio/flac",
            Self::Wav => "audio/wav",
            Self::Mp3 => "audio/mpeg",
            Self::Aac => "audio/aac",
            Self::Webm => "audio/webm",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermes_audio_detection_uses_container_bytes_instead_of_misleading_extensions() {
        for (bytes, kind) in [
            (b"\x00\x00\x00\x18ftypM4A ".as_slice(), Container::M4a),
            (b"\x00\x00\x00\x18ftypisom".as_slice(), Container::Mp4),
            (b"OggS".as_slice(), Container::Ogg),
            (b"fLaC".as_slice(), Container::Flac),
            (b"RIFF\x00\x00\x00\x00WAVE".as_slice(), Container::Wav),
            (b"ID3".as_slice(), Container::Mp3),
            (b"\xff\xfb".as_slice(), Container::Mp3),
            (b"\xff\xf1".as_slice(), Container::Aac),
            (b"\x1a\x45\xdf\xa3".as_slice(), Container::Webm),
        ] {
            assert_eq!(Container::sniff(bytes), Some(kind));
        }
        for bytes in [b"".as_slice(), b"\xff".as_slice(), b"RIFF\0\0\0\0WEBP".as_slice(), b"%PDF".as_slice()] {
            assert_eq!(Container::sniff(bytes), None);
        }
    }
}
