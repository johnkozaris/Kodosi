use serde::{Deserialize, Serialize};

/// Every viewer can decode H.264, so it is the codec a stream falls back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    H264,
    Hevc,
}

impl Codec {
    pub const fn wire(self) -> u8 {
        match self {
            Self::H264 => 0,
            Self::Hevc => 1,
        }
    }

    pub const fn from_wire(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::H264),
            1 => Some(Self::Hevc),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decoder {
    pub codec: Codec,
    #[serde(default)]
    pub hardware: bool,
}

/// What the encoder produces for the viewers of a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Format {
    pub codec: Codec,
    /// Long-term references that let a viewer recover from an anchor instead of a keyframe.
    /// Some hardware decoders freeze on them, so only viewers that declare support get them.
    pub anchors: bool,
}

impl Default for Format {
    fn default() -> Self {
        Self {
            codec: Codec::H264,
            anchors: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Viewing<'a> {
    pub decoders: &'a [Decoder],
    pub anchors: bool,
}

/// HEVC needs every viewer to decode it in hardware, since software HEVC decoding costs more
/// than the bits it saves.
pub fn choose<'a>(encodable: &[Codec], viewers: impl IntoIterator<Item = Viewing<'a>>) -> Format {
    let mut hevc = encodable.contains(&Codec::Hevc);
    let mut anchors = true;
    for viewing in viewers {
        hevc &= viewing
            .decoders
            .iter()
            .any(|decoder| decoder.codec == Codec::Hevc && decoder.hardware);
        anchors &= viewing.anchors;
    }
    Format {
        codec: if hevc { Codec::Hevc } else { Codec::H264 },
        anchors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H264: Decoder = Decoder {
        codec: Codec::H264,
        hardware: true,
    };
    const HEVC: Decoder = Decoder {
        codec: Codec::Hevc,
        hardware: true,
    };
    const SOFTWARE_HEVC: Decoder = Decoder {
        codec: Codec::Hevc,
        hardware: false,
    };

    fn viewing(decoders: &[Decoder], anchors: bool) -> Viewing<'_> {
        Viewing { decoders, anchors }
    }

    #[test]
    fn hevc_streams_only_when_the_source_encodes_it_and_every_viewer_decodes_it_in_hardware() {
        let both = [Codec::H264, Codec::Hevc];
        let apple = [H264, HEVC];
        let qt = [H264];
        let slow = [H264, SOFTWARE_HEVC];
        assert_eq!(choose(&both, [viewing(&apple, true)]).codec, Codec::Hevc);
        assert_eq!(
            choose(&both, [viewing(&apple, true), viewing(&qt, false)]).codec,
            Codec::H264
        );
        assert_eq!(choose(&both, [viewing(&slow, false)]).codec, Codec::H264);
        assert_eq!(
            choose(&[Codec::H264], [viewing(&apple, true)]).codec,
            Codec::H264
        );
    }

    #[test]
    fn anchors_need_every_viewer() {
        let apple = [H264, HEVC];
        assert!(choose(&[Codec::H264], [viewing(&apple, true)]).anchors);
        assert!(
            !choose(
                &[Codec::H264],
                [viewing(&apple, true), viewing(&[H264], false)]
            )
            .anchors
        );
    }
}
