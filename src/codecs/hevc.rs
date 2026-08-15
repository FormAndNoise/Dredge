#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HevcConfig {
    pub general_profile_idc: u8,
    pub general_level_idc: u8,
    pub nal_length_size: usize,
    pub vps_list: Vec<Vec<u8>>,
    pub sps_list: Vec<Vec<u8>>,
    pub pps_list: Vec<Vec<u8>>,
}

impl HevcConfig {
    /// Parse hvcC box payload (ISO/IEC 14496-15 section 8.3.3.1)
    pub fn parse_hvcc(payload: &[u8]) -> Option<Self> {
        if payload.len() < 23 {
            return None;
        }

        let version = payload[0];
        if version != 1 {
            return None;
        }

        let general_profile_idc = payload[1] & 0x1F;
        let general_level_idc = payload[12];
        let nal_length_size = ((payload[21] & 0x03) + 1) as usize;
        let num_arrays = payload[22] as usize;

        let mut offset = 23usize;
        let mut vps_list = Vec::new();
        let mut sps_list = Vec::new();
        let mut pps_list = Vec::new();

        for _ in 0..num_arrays {
            if offset + 3 > payload.len() {
                break;
            }
            let nal_unit_type = payload[offset] & 0x3F;
            let num_nalus = u16::from_be_bytes([payload[offset + 1], payload[offset + 2]]) as usize;
            offset += 3;

            for _ in 0..num_nalus {
                if offset + 2 > payload.len() {
                    return None;
                }
                let nalu_len = u16::from_be_bytes([payload[offset], payload[offset + 1]]) as usize;
                offset += 2;
                if offset + nalu_len > payload.len() {
                    return None;
                }
                let nal_bytes = payload[offset..offset + nalu_len].to_vec();
                offset += nalu_len;

                match nal_unit_type {
                    32 => vps_list.push(nal_bytes), // VPS
                    33 => sps_list.push(nal_bytes), // SPS
                    34 => pps_list.push(nal_bytes), // PPS
                    _ => {}
                }
            }
        }

        Some(Self {
            general_profile_idc,
            general_level_idc,
            nal_length_size,
            vps_list,
            sps_list,
            pps_list,
        })
    }

    /// Build Annex B sequence header (VPS + SPS + PPS with 00 00 00 01 start codes)
    pub fn to_annex_b_header(&self) -> Vec<u8> {
        let mut header = Vec::new();
        for vps in &self.vps_list {
            header.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
            header.extend_from_slice(vps);
        }
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
