#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AacConfig {
    pub audio_object_type: u8,
    pub sample_rate_index: u8,
    pub channel_config: u8,
    pub sample_rate: u32,
}

impl Default for AacConfig {
    fn default() -> Self {
        Self {
            audio_object_type: 2,    // AAC-LC
            sample_rate_index: 4,    // 44100 Hz
            channel_config: 2,       // Stereo
            sample_rate: 44100,
        }
    }
}

pub const AAC_SAMPLE_RATES: [u32; 13] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
];

impl AacConfig {
    /// Parse AudioSpecificConfig from raw 2+ bytes
    pub fn from_audio_specific_config(data: &[u8]) -> Option<Self> {
        if data.len() < 2 {
            return None;
        }

        let audio_object_type = (data[0] >> 3) & 0x1F;
        let sample_rate_index = ((data[0] & 0x07) << 1) | ((data[1] >> 7) & 0x01);
        let channel_config = (data[1] >> 3) & 0x0F;

        let sample_rate = if (sample_rate_index as usize) < AAC_SAMPLE_RATES.len() {
            AAC_SAMPLE_RATES[sample_rate_index as usize]
        } else {
            44100
        };

        Some(Self {
            audio_object_type: if audio_object_type == 0 { 2 } else { audio_object_type },
            sample_rate_index,
            channel_config: if channel_config == 0 { 2 } else { channel_config },
            sample_rate,
        })
    }

    /// Parse esds box payload (ISO/IEC 14496-1)
    pub fn parse_esds(payload: &[u8]) -> Option<Self> {
        // Look for DecSpecificInfoTag (0x05)
        let mut cursor = 0usize;
        // Skip FullBox 4 bytes header if present
        if payload.len() >= 4 && (payload[0] == 0 || payload[0] == 1) {
            cursor = 4;
        }

        while cursor < payload.len() {
            let tag = payload[cursor];
            cursor += 1;

            // Read tag length (can be variable-length encoded with high bit set)
            let mut length = 0usize;
            for _ in 0..4 {
                if cursor >= payload.len() {
                    return None;
                }
                let b = payload[cursor];
                cursor += 1;
                length = (length << 7) | (b & 0x7F) as usize;
                if b & 0x80 == 0 {
                    break;
                }
            }

            if tag == 0x05 {
                // DecSpecificInfo contains AudioSpecificConfig
                if cursor + length <= payload.len() {
                    return Self::from_audio_specific_config(&payload[cursor..cursor + length]);
                }
            } else if tag == 0x03 {
                // ES_Descriptor has 2 bytes ES_ID + 1 byte flags before nested descriptors
                cursor += 3;
            } else if tag == 0x04 {
                // DecoderConfigDescriptor has 13 bytes fixed fields before DecSpecificInfoTag
                cursor += 13;
            }
        }

        None
    }

    /// Synthesize standard 7-byte ADTS header for a raw AAC access unit
    pub fn synthesize_adts_header(&self, frame_payload_len: usize) -> [u8; 7] {
        let total_frame_len = frame_payload_len + 7;
        let mut hdr = [0u8; 7];

        let aot = self.audio_object_type;
        let freq_idx = self.sample_rate_index & 0x0F;
        let chan_cfg = self.channel_config & 0x0F;

        // Byte 0: 0xFF (Sync word bits 0-7)
        hdr[0] = 0xFF;

        // Byte 1: 0xF1 (Sync word bits 8-11: 1111, ID: 0 for MPEG-4 / 1 for MPEG-2, Layer: 00, Protection absent: 1)
        hdr[1] = 0xF1;

        // Byte 2: Profile (2 bits: AOT - 1), freq_idx (4 bits), private (1 bit: 0), chan_cfg bit 2 (1 bit)
        let profile = (aot.saturating_sub(1)) & 0x03;
        hdr[2] = (profile << 6) | (freq_idx << 2) | ((chan_cfg >> 2) & 0x01);

        // Byte 3: chan_cfg bits 0-1 (2 bits), orig/copy (1 bit), home (1 bit), copyright (1 bit), copy start (1 bit), frame_len bits 11-12 (2 bits)
        hdr[3] = ((chan_cfg & 0x03) << 6) | (((total_frame_len >> 11) & 0x03) as u8);

        // Byte 4: frame_len bits 3-10
        hdr[4] = ((total_frame_len >> 3) & 0xFF) as u8;

        // Byte 5: frame_len bits 0-2 (3 bits), buffer fullness (5 bits: 0x1F)
        hdr[5] = (((total_frame_len & 0x07) as u8) << 5) | 0x1F;

        // Byte 6: buffer fullness remaining (6 bits: 0x3F), number of AAC frames minus 1 (2 bits: 00)
        hdr[6] = 0xFC;

        hdr
    }
}
