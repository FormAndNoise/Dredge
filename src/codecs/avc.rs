#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvcConfig {
    pub profile_idc: u8,
    pub profile_compatibility: u8,
    pub level_idc: u8,
    pub nal_length_size: usize,
    pub sps_list: Vec<Vec<u8>>,
    pub pps_list: Vec<Vec<u8>>,
}

impl AvcConfig {
    /// Parse avcC box payload (ISO/IEC 14496-15 section 5.2.4.1)
    pub fn parse_avcc(payload: &[u8]) -> Option<Self> {
        if payload.len() < 7 {
            return None;
        }

        let version = payload[0];
        if version != 1 {
            return None;
        }

        let profile_idc = payload[1];
        let profile_compatibility = payload[2];
        let level_idc = payload[3];
        let nal_length_size = ((payload[4] & 0x03) + 1) as usize;
        let num_sps = (payload[5] & 0x1F) as usize;

        let mut offset = 6usize;
        let mut sps_list = Vec::new();

        for _ in 0..num_sps {
            if offset + 2 > payload.len() {
                return None;
            }
            let sps_len = u16::from_be_bytes([payload[offset], payload[offset + 1]]) as usize;
            offset += 2;
            if offset + sps_len > payload.len() {
                return None;
            }
            sps_list.push(payload[offset..offset + sps_len].to_vec());
            offset += sps_len;
        }

        if offset >= payload.len() {
            return None;
        }

        let num_pps = payload[offset] as usize;
        offset += 1;

        let mut pps_list = Vec::new();
        for _ in 0..num_pps {
            if offset + 2 > payload.len() {
                return None;
            }
            let pps_len = u16::from_be_bytes([payload[offset], payload[offset + 1]]) as usize;
            offset += 2;
            if offset + pps_len > payload.len() {
                return None;
            }
            pps_list.push(payload[offset..offset + pps_len].to_vec());
            offset += pps_len;
        }

        Some(Self {
            profile_idc,
            profile_compatibility,
            level_idc,
            nal_length_size,
            sps_list,
            pps_list,
        })
    }

    /// Build Annex B sequence header (SPS + PPS with 00 00 00 01 start codes)
    pub fn to_annex_b_header(&self) -> Vec<u8> {
        let mut header = Vec::new();
        for sps in &self.sps_list {
            header.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
            header.extend_from_slice(sps);
        }
        for pps in &self.pps_list {
            header.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
            header.extend_from_slice(pps);
        }
        header
    }
}
