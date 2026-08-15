use std::io::{self, Read, Seek, SeekFrom};

#[derive(Debug, Clone)]
pub struct Mp4Box {
    pub box_type: [u8; 4],
    pub offset: u64,
    pub header_size: u64,
    pub payload_size: u64,
    pub payload: Vec<u8>,
    pub children: Vec<Mp4Box>,
}

impl Mp4Box {
    pub fn type_name(&self) -> String {
        String::from_utf8_lossy(&self.box_type).to_string()
    }

    pub fn find_child(&self, box_type: &[u8; 4]) -> Option<&Mp4Box> {
        self.children.iter().find(|c| &c.box_type == box_type)
    }

    pub fn find_descendant(&self, box_type: &[u8; 4]) -> Option<&Mp4Box> {
        if &self.box_type == box_type {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_descendant(box_type) {
                return Some(found);
            }
        }
        None
    }

    pub fn find_all_descendants<'a>(&'a self, box_type: &[u8; 4], out: &mut Vec<&'a Mp4Box>) {
        if &self.box_type == box_type {
            out.push(self);
        }
        for child in &self.children {
            child.find_all_descendants(box_type, out);
        }
    }
}

pub fn is_container_box(box_type: &[u8; 4]) -> bool {
    matches!(
        box_type,
        b"moov"
            | b"trak"
            | b"mdia"
            | b"minf"
            | b"stbl"
            | b"edts"
            | b"dinf"
            | b"mvex"
            | b"moof"
            | b"traf"
            | b"udta"
            | b"clip"
            | b"matt"
            | b"kmat"
            | b"wave"
    )
}

pub fn is_visual_sample_entry(box_type: &[u8; 4]) -> bool {
    matches!(
        box_type,
        b"avc1" | b"avc3" | b"hvc1" | b"hev1" | b"mp4v" | b"encv" | b"dvh1" | b"dvhe"
    )
}

pub fn is_audio_sample_entry(box_type: &[u8; 4]) -> bool {
    matches!(
        box_type,
        b"mp4a" | b"enca" | b"samr" | b"sawb" | b"alac" | b"ac-3" | b"ec-3" | b"lpcm" | b"twos" | b"sowt"
    )
}

/// Recursively parse ISO BMFF boxes from a slice
pub fn parse_boxes(data: &[u8], base_offset: u64) -> Vec<Mp4Box> {
    let mut boxes = Vec::new();
    let mut cursor = 0usize;

    while cursor + 8 <= data.len() {
        let box_start = cursor;
        let size32 = u32::from_be_bytes([
            data[cursor],
            data[cursor + 1],
            data[cursor + 2],
            data[cursor + 3],
        ]) as u64;
        let box_type = [
            data[cursor + 4],
            data[cursor + 5],
            data[cursor + 6],
            data[cursor + 7],
        ];

        let mut header_size = 8u64;
        let total_size = if size32 == 1 {
            // 64-bit large size
            if cursor + 16 > data.len() {
                break;
            }
            header_size = 16;
            u64::from_be_bytes([
                data[cursor + 8],
                data[cursor + 9],
                data[cursor + 10],
                data[cursor + 11],
                data[cursor + 12],
                data[cursor + 13],
                data[cursor + 14],
                data[cursor + 15],
            ])
        } else if size32 == 0 {
            // Extends to end of slice
            (data.len() - cursor) as u64
        } else {
            size32
        };

        if total_size < header_size {
            break;
        }

        let payload_size = total_size - header_size;
        let payload_start = cursor + header_size as usize;
        let payload_end = (cursor as u64 + total_size).min(data.len() as u64) as usize;

        if payload_start > data.len() {
            break;
        }

        let payload = data[payload_start..payload_end].to_vec();
        let abs_offset = base_offset + box_start as u64;

        let mut child_boxes = Vec::new();

        if is_container_box(&box_type) {
            child_boxes = parse_boxes(&payload, abs_offset + header_size);
        } else if &box_type == b"stsd" {
            // stsd is a FullBox: 4 bytes version/flags + 4 bytes entry count
            if payload.len() >= 8 {
                let inner_payload = &payload[8..];
                child_boxes = parse_boxes(inner_payload, abs_offset + header_size + 8);
            }
        } else if is_visual_sample_entry(&box_type) {
            // VisualSampleEntry has 78 bytes of fixed fields before child boxes
            if payload.len() > 78 {
                let inner_payload = &payload[78..];
                child_boxes = parse_boxes(inner_payload, abs_offset + header_size + 78);
            }
        } else if is_audio_sample_entry(&box_type) {
            // AudioSampleEntry version 0 has 28 bytes fixed header
            if payload.len() > 28 {
                let inner_payload = &payload[28..];
                child_boxes = parse_boxes(inner_payload, abs_offset + header_size + 28);
            }
        }

        boxes.push(Mp4Box {
            box_type,
            offset: abs_offset,
            header_size,
            payload_size,
            payload,
            children: child_boxes,
        });

        cursor += total_size as usize;
        if total_size == 0 {
            break;
        }
    }

    boxes
}

/// Recursively parse boxes directly from a Seekable Read stream without loading full file into memory
pub fn parse_file_boxes<R: Read + Seek>(reader: &mut R) -> io::Result<Vec<Mp4Box>> {
    let file_len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;

    let mut root_boxes = Vec::new();
    let mut current_offset = 0u64;

    while current_offset + 8 <= file_len {
        reader.seek(SeekFrom::Start(current_offset))?;
        let mut hdr_buf = [0u8; 8];
        if reader.read_exact(&mut hdr_buf).is_err() {
            break;
        }

        let size32 = u32::from_be_bytes([hdr_buf[0], hdr_buf[1], hdr_buf[2], hdr_buf[3]]) as u64;
        let box_type = [hdr_buf[4], hdr_buf[5], hdr_buf[6], hdr_buf[7]];

        let mut header_size = 8u64;
        let total_size = if size32 == 1 {
            let mut large_buf = [0u8; 8];
            reader.read_exact(&mut large_buf)?;
            header_size = 16;
            u64::from_be_bytes(large_buf)
        } else if size32 == 0 {
            file_len - current_offset
        } else {
            size32
        };

        if total_size < header_size {
            break;
        }

        let payload_size = total_size - header_size;
        let payload_offset = current_offset + header_size;

        // Only read payload into memory for non-mdat boxes or metadata boxes <= 32MB
        let mut payload = Vec::new();
        let mut child_boxes = Vec::new();

        if &box_type != b"mdat" && payload_size <= 32 * 1024 * 1024 {
            payload.resize(payload_size as usize, 0);
            reader.seek(SeekFrom::Start(payload_offset))?;
            reader.read_exact(&mut payload)?;

            if is_container_box(&box_type) {
                child_boxes = parse_boxes(&payload, payload_offset);
            } else if &box_type == b"stsd" && payload.len() >= 8 {
                child_boxes = parse_boxes(&payload[8..], payload_offset + 8);
            } else if is_visual_sample_entry(&box_type) && payload.len() > 78 {
                child_boxes = parse_boxes(&payload[78..], payload_offset + 78);
            } else if is_audio_sample_entry(&box_type) && payload.len() > 28 {
                child_boxes = parse_boxes(&payload[28..], payload_offset + 28);
            }
        }

        root_boxes.push(Mp4Box {
            box_type,
            offset: current_offset,
            header_size,
            payload_size,
            payload,
            children: child_boxes,
        });

        current_offset += total_size;
    }

    Ok(root_boxes)
}
