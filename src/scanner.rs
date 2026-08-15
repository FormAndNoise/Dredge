use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::profile::DredgeProfile;

const CHUNK_SIZE: usize = 8 * 1024 * 1024; // 8MB read chunk
const OVERLAP_SIZE: usize = 512 * 1024;     // 512KB overlap buffer

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFormat {
    H264,
    Hevc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Aac,
    Wav,
    Mp3,
    Raw,
}

#[derive(Debug, Default, Clone)]
pub struct ScanOptions {
    pub start_offset: Option<u64>,
    pub end_offset: Option<u64>,
    pub verbose: bool,
}

#[derive(Debug, Default)]
pub struct RecoveryStats {
    pub video_bytes_written: u64,
    pub video_frames_recovered: u64,
    pub audio_bytes_written: u64,
    pub audio_frames_recovered: u64,
}

/// Find starting offset of media payload (mdat box or user offset)
pub fn find_media_start_offset<P: AsRef<Path>>(path: P, user_start: Option<u64>) -> io::Result<u64> {
    if let Some(start) = user_start {
        return Ok(start);
    }

    let mut file = File::open(path)?;
    let file_len = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(0))?;

    let mut offset = 0u64;
    while offset + 8 <= file_len {
        file.seek(SeekFrom::Start(offset))?;
        let mut hdr = [0u8; 8];
        if file.read_exact(&mut hdr).is_err() {
            break;
        }

        let size32 = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as u64;
        let box_type = &hdr[4..8];

        if box_type == b"mdat" {
            return Ok(offset + 8);
        }

        let size = if size32 == 1 {
            let mut large_buf = [0u8; 8];
            file.read_exact(&mut large_buf)?;
            u64::from_be_bytes(large_buf)
        } else if size32 == 0 {
            file_len - offset
        } else {
            size32
        };

        if size < 8 {
            break;
        }
        offset += size;
    }

    // Default to start of file if no mdat header found
    Ok(0)
}

/// Recover video elementary stream (H.264 or HEVC) with Annex B start codes
pub fn recover_video<P: AsRef<Path>, Q: AsRef<Path>>(
    input_path: P,
    output_path: Q,
    format: VideoFormat,
    profile: Option<&DredgeProfile>,
    options: &ScanOptions,
) -> io::Result<RecoveryStats> {
    let mut stats = RecoveryStats::default();

    let mut in_file = File::open(input_path.as_ref())?;
    let file_len = in_file.seek(SeekFrom::End(0))?;

    let start_offset = find_media_start_offset(input_path.as_ref(), options.start_offset)?;
    let end_offset = match options.end_offset {
        Some(0) | None => file_len,
        Some(end) => end.min(file_len),
    };

    let out_file = File::create(output_path.as_ref())?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, out_file);

    // 1. Write reconstructed SPS/PPS parameter set header from profile
    if let Some(prof) = profile {
        if let Some(v_prof) = &prof.video {
            match format {
                VideoFormat::H264 => {
                    if let Some(avc) = &v_prof.avc {
                        let header = avc.to_annex_b_header();
                        if !header.is_empty() {
                            writer.write_all(&header)?;
                            stats.video_bytes_written += header.len() as u64;
                            if options.verbose {
                                eprintln!("[Dredge] Prepended {} bytes of Annex B SPS/PPS parameter sets", header.len());
                            }
                        }
                    }
                }
                VideoFormat::Hevc => {
                    if let Some(hevc) = &v_prof.hevc {
                        let header = hevc.to_annex_b_header();
                        if !header.is_empty() {
                            writer.write_all(&header)?;
                            stats.video_bytes_written += header.len() as u64;
                            if options.verbose {
                                eprintln!("[Dredge] Prepended {} bytes of Annex B VPS/SPS/PPS parameter sets", header.len());
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Stream through file in chunks and extract NAL units
    let nal_len_size = profile
        .and_then(|p| p.video.as_ref())
        .map(|v| v.nal_length_size)
        .unwrap_or(4);

    let mut current_pos = start_offset;
    in_file.seek(SeekFrom::Start(current_pos))?;

    let mut buffer = vec![0u8; CHUNK_SIZE + OVERLAP_SIZE];
    let mut buf_len = 0usize;

    loop {
        let max_to_read = ((CHUNK_SIZE) as u64).min(end_offset.saturating_sub(current_pos + buf_len as u64)) as usize;
        let mut is_eof = true;

        if max_to_read > 0 {
            let read_bytes = in_file.read(&mut buffer[buf_len..buf_len + max_to_read])?;
            if read_bytes > 0 {
                buf_len += read_bytes;
                is_eof = false;
            }
        }

        if buf_len == 0 {
            break;
        }

        let slice = &buffer[..buf_len];
        let mut cursor = 0usize;

        while cursor + 4 <= buf_len {
            // Check for 3-byte or 4-byte Annex B start code
            if slice[cursor] == 0 && slice[cursor + 1] == 0 {
                if slice[cursor + 2] == 1 {
                    // 3-byte start code 00 00 01
                    writer.write_all(&[0x00, 0x00, 0x00, 0x01])?;
                    cursor += 3;
                    stats.video_bytes_written += 4;
                    stats.video_frames_recovered += 1;
                    continue;
                } else if cursor + 3 < buf_len && slice[cursor + 2] == 0 && slice[cursor + 3] == 1 {
                    // 4-byte start code 00 00 00 01
                    writer.write_all(&[0x00, 0x00, 0x00, 0x01])?;
                    cursor += 4;
                    stats.video_bytes_written += 4;
                    stats.video_frames_recovered += 1;
                    continue;
                }
            }

            // Check for MP4 length-prefixed NAL unit
            if cursor + nal_len_size <= buf_len {
                let nalu_size = match nal_len_size {
                    4 => u32::from_be_bytes([
                        slice[cursor],
                        slice[cursor + 1],
                        slice[cursor + 2],
                        slice[cursor + 3],
                    ]) as usize,
                    2 => u16::from_be_bytes([slice[cursor], slice[cursor + 1]]) as usize,
                    1 => slice[cursor] as usize,
                    _ => 0,
                };

                if nalu_size >= 2 && nalu_size <= 8 * 1024 * 1024 && cursor + nal_len_size + nalu_size <= buf_len {
                    let nal_payload = &slice[cursor + nal_len_size..cursor + nal_len_size + nalu_size];
                    let nal_type = match format {
                        VideoFormat::H264 => nal_payload[0] & 0x1F,
                        VideoFormat::Hevc => (nal_payload[0] >> 1) & 0x3F,
                    };

                    let valid_nal = match format {
                        VideoFormat::H264 => nal_type >= 1 && nal_type <= 19,
                        VideoFormat::Hevc => nal_type <= 39,
                    };

                    if valid_nal {
                        writer.write_all(&[0x00, 0x00, 0x00, 0x01])?;
                        writer.write_all(nal_payload)?;
                        stats.video_bytes_written += 4 + nalu_size as u64;
                        stats.video_frames_recovered += 1;
                        cursor += nal_len_size + nalu_size;
                        continue;
                    }
                }
            }

            cursor += 1;
        }

        current_pos += cursor as u64;

        if is_eof {
            break;
        }

        // Shift remaining unprocessed tail to start of buffer
        let remaining = buf_len.saturating_sub(cursor);
        if remaining > 0 && remaining <= OVERLAP_SIZE {
            buffer.copy_within(cursor..buf_len, 0);
            buf_len = remaining;
        } else {
            buf_len = 0;
        }
    }

    writer.flush()?;
    Ok(stats)
}

/// Recover audio elementary stream (AAC with synthesized ADTS, MP3, or PCM)
pub fn recover_audio<P: AsRef<Path>, Q: AsRef<Path>>(
    input_path: P,
    output_path: Q,
    format: AudioFormat,
    profile: Option<&DredgeProfile>,
    options: &ScanOptions,
) -> io::Result<RecoveryStats> {
    let mut stats = RecoveryStats::default();

    let mut in_file = File::open(input_path.as_ref())?;
    let file_len = in_file.seek(SeekFrom::End(0))?;

    let start_offset = find_media_start_offset(input_path.as_ref(), options.start_offset)?;
    let end_offset = match options.end_offset {
        Some(0) | None => file_len,
        Some(end) => end.min(file_len),
    };

    let out_file = File::create(output_path.as_ref())?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, out_file);

    let _aac_config = profile
        .and_then(|p| p.audio.as_ref())
        .and_then(|a| a.aac.as_ref())
        .cloned()
        .unwrap_or_default();

    let mut current_pos = start_offset;
    in_file.seek(SeekFrom::Start(current_pos))?;

    let mut buffer = vec![0u8; CHUNK_SIZE + OVERLAP_SIZE];
    let mut buf_len = 0usize;

    match format {
        AudioFormat::Aac => {
            loop {
                let max_to_read = ((CHUNK_SIZE) as u64).min(end_offset.saturating_sub(current_pos + buf_len as u64)) as usize;
                let mut is_eof = true;

                if max_to_read > 0 {
                    let read_bytes = in_file.read(&mut buffer[buf_len..buf_len + max_to_read])?;
                    if read_bytes > 0 {
                        buf_len += read_bytes;
                        is_eof = false;
                    }
                }

                if buf_len == 0 {
                    break;
                }

                let slice = &buffer[..buf_len];
                let mut cursor = 0usize;

                while cursor + 7 <= buf_len {
                    if slice[cursor] == 0xFF && (slice[cursor + 1] & 0xF0) == 0xF0 {
                        let frame_len = (((slice[cursor + 3] & 0x03) as usize) << 11)
                            | ((slice[cursor + 4] as usize) << 3)
                            | ((slice[cursor + 5] >> 5) as usize);

                        if frame_len >= 7 && frame_len <= 8192 && cursor + frame_len <= buf_len {
                            writer.write_all(&slice[cursor..cursor + frame_len])?;
                            stats.audio_bytes_written += frame_len as u64;
                            stats.audio_frames_recovered += 1;
                            cursor += frame_len;
                            continue;
                        }
                    }

                    cursor += 1;
                }

                current_pos += cursor as u64;

                if is_eof {
                    break;
                }

                let remaining = buf_len.saturating_sub(cursor);
                if remaining > 0 && remaining <= OVERLAP_SIZE {
                    buffer.copy_within(cursor..buf_len, 0);
                    buf_len = remaining;
                } else {
                    buf_len = 0;
                }
            }
        }
        AudioFormat::Mp3 => {
            loop {
                let max_to_read = ((CHUNK_SIZE) as u64).min(end_offset.saturating_sub(current_pos + buf_len as u64)) as usize;
                let mut is_eof = true;

                if max_to_read > 0 {
                    let read_bytes = in_file.read(&mut buffer[buf_len..buf_len + max_to_read])?;
                    if read_bytes > 0 {
                        buf_len += read_bytes;
                        is_eof = false;
                    }
                }

                if buf_len == 0 {
                    break;
                }

                let slice = &buffer[..buf_len];
                let mut cursor = 0usize;

                while cursor + 4 <= buf_len {
                    if slice[cursor] == 0xFF && (slice[cursor + 1] & 0xE0) == 0xE0 {
                        let layer = (slice[cursor + 1] >> 1) & 0x03;
                        let bitrate_idx = (slice[cursor + 2] >> 4) & 0x0F;
                        let srate_idx = (slice[cursor + 2] >> 2) & 0x03;
                        let padding = (slice[cursor + 2] >> 1) & 0x01;

                        if layer == 1 && bitrate_idx > 0 && bitrate_idx < 15 && srate_idx < 3 {
                            let bitrates = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320];
                            let srates = [44100, 48000, 32000];

                            let br = bitrates[bitrate_idx as usize] * 1000;
                            let sr = srates[srate_idx as usize];
                            let frame_len = (144 * br / sr) + padding as usize;

                            if frame_len >= 24 && cursor + frame_len <= buf_len {
                                writer.write_all(&slice[cursor..cursor + frame_len])?;
                                stats.audio_bytes_written += frame_len as u64;
                                stats.audio_frames_recovered += 1;
                                cursor += frame_len;
                                continue;
                            }
                        }
                    }
                    cursor += 1;
                }

                current_pos += cursor as u64;

                if is_eof {
                    break;
                }

                let remaining = buf_len.saturating_sub(cursor);
                if remaining > 0 && remaining <= OVERLAP_SIZE {
                    buffer.copy_within(cursor..buf_len, 0);
                    buf_len = remaining;
                } else {
                    buf_len = 0;
                }
            }
        }
        AudioFormat::Wav | AudioFormat::Raw => {
            let sample_rate = profile.and_then(|p| p.audio.as_ref()).map(|a| a.sample_rate).unwrap_or(44100);
            let channels = profile.and_then(|p| p.audio.as_ref()).map(|a| a.channels).unwrap_or(2);
            let bits_per_sample = 16u16;

            if format == AudioFormat::Wav {
                let byte_rate = sample_rate * channels as u32 * (bits_per_sample as u32 / 8);
                let block_align = channels * (bits_per_sample / 8);

                writer.write_all(b"RIFF\x00\x00\x00\x00WAVEfmt ")?;
                writer.write_all(&16u32.to_le_bytes())?;
                writer.write_all(&1u16.to_le_bytes())?;
                writer.write_all(&channels.to_le_bytes())?;
                writer.write_all(&sample_rate.to_le_bytes())?;
                writer.write_all(&byte_rate.to_le_bytes())?;
                writer.write_all(&block_align.to_le_bytes())?;
                writer.write_all(&bits_per_sample.to_le_bytes())?;
                writer.write_all(b"data\x00\x00\x00\x00")?;
                stats.audio_bytes_written += 44;
            }

            while current_pos < end_offset {
                let bytes_to_read = (CHUNK_SIZE as u64).min(end_offset - current_pos) as usize;
                if bytes_to_read == 0 {
                    break;
                }
                let read_bytes = in_file.read(&mut buffer[..bytes_to_read])?;
                if read_bytes == 0 {
                    break;
                }
                writer.write_all(&buffer[..read_bytes])?;
                stats.audio_bytes_written += read_bytes as u64;
                current_pos += read_bytes as u64;
            }
        }
    }

    writer.flush()?;
    Ok(stats)
}
