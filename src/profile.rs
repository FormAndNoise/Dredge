use std::fs::File;
use std::io::{self, Read, Seek, Write};
use std::path::Path;

use crate::codecs::{AacConfig, AvcConfig, HevcConfig};
use crate::mp4_box::parse_file_boxes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoProfile {
    pub codec: String, // "avc1" or "hvc1"
    pub width: u32,
    pub height: u32,
    pub timescale: u32,
    pub nal_length_size: usize,
    pub avc: Option<AvcConfig>,
    pub hevc: Option<HevcConfig>,
    pub raw_config_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioProfile {
    pub codec: String, // "mp4a", "mp3", "pcm"
    pub sample_rate: u32,
    pub channels: u16,
    pub timescale: u32,
    pub aac: Option<AacConfig>,
    pub raw_config_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DredgeProfile {
    pub source_filename: String,
    pub file_size: u64,
    pub major_brand: String,
    pub video: Option<VideoProfile>,
    pub audio: Option<AudioProfile>,
}

impl DredgeProfile {
    /// Analyze a working reference MP4/MOV file and construct a recovery profile
    pub fn analyze_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path_ref = path.as_ref();
        let filename = path_ref
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "reference.mp4".to_string());

        let mut file = File::open(path_ref)?;
        let file_size = file.seek(io::SeekFrom::End(0))?;
        file.seek(io::SeekFrom::Start(0))?;

        let boxes = parse_file_boxes(&mut file)?;

        let mut major_brand = "isom".to_string();
        if let Some(ftyp) = boxes.iter().find(|b| &b.box_type == b"ftyp") {
            if ftyp.payload.len() >= 4 {
                major_brand = String::from_utf8_lossy(&ftyp.payload[0..4]).trim().to_string();
            }
        }

        let mut video_profile = None;
        let mut audio_profile = None;

        // Search for track boxes
        let mut track_boxes = Vec::new();
        for b in &boxes {
            b.find_all_descendants(b"trak", &mut track_boxes);
        }

        for trak in track_boxes {
            // Check handler type (vide or soun)
            let mut is_video = false;
            let mut is_audio = false;

            if let Some(hdlr) = trak.find_descendant(b"hdlr") {
                if hdlr.payload.len() >= 12 {
                    let subtype = &hdlr.payload[8..12];
                    if subtype == b"vide" {
                        is_video = true;
                    } else if subtype == b"soun" {
                        is_audio = true;
                    }
                }
            }

            // Timescale from mdhd
            let mut timescale = 30000u32;
            if let Some(mdhd) = trak.find_descendant(b"mdhd") {
                if mdhd.payload.len() >= 16 {
                    let version = mdhd.payload[0];
                    if version == 0 && mdhd.payload.len() >= 16 {
                        timescale = u32::from_be_bytes([
                            mdhd.payload[12],
                            mdhd.payload[13],
                            mdhd.payload[14],
                            mdhd.payload[15],
                        ]);
                    } else if version == 1 && mdhd.payload.len() >= 24 {
                        timescale = u32::from_be_bytes([
                            mdhd.payload[20],
                            mdhd.payload[21],
                            mdhd.payload[22],
                            mdhd.payload[23],
                        ]);
                    }
                }
            }

            // Dimensions from tkhd
            let mut width = 1920u32;
            let mut height = 1080u32;
            if let Some(tkhd) = trak.find_descendant(b"tkhd") {
                if tkhd.payload.len() >= 84 {
                    let version = tkhd.payload[0];
                    let offset = if version == 0 { 76 } else { 88 };
                    if tkhd.payload.len() >= offset + 8 {
                        let w_fixed = u32::from_be_bytes([
                            tkhd.payload[offset],
                            tkhd.payload[offset + 1],
                            tkhd.payload[offset + 2],
                            tkhd.payload[offset + 3],
                        ]);
                        let h_fixed = u32::from_be_bytes([
                            tkhd.payload[offset + 4],
                            tkhd.payload[offset + 5],
                            tkhd.payload[offset + 6],
                            tkhd.payload[offset + 7],
                        ]);
                        width = w_fixed >> 16;
                        height = h_fixed >> 16;
                    }
                }
            }

            // Check for AVC / H.264
            if let Some(avcc_box) = trak.find_descendant(b"avcC") {
                let avc_cfg = AvcConfig::parse_avcc(&avcc_box.payload);
                let nal_len = avc_cfg.as_ref().map(|c| c.nal_length_size).unwrap_or(4);
                let raw_hex = to_hex(&avcc_box.payload);

                video_profile = Some(VideoProfile {
                    codec: "avc1".to_string(),
                    width,
                    height,
                    timescale,
                    nal_length_size: nal_len,
                    avc: avc_cfg,
                    hevc: None,
                    raw_config_hex: raw_hex,
                });
            } else if let Some(hvcc_box) = trak.find_descendant(b"hvcC") {
                let hevc_cfg = HevcConfig::parse_hvcc(&hvcc_box.payload);
                let nal_len = hevc_cfg.as_ref().map(|c| c.nal_length_size).unwrap_or(4);
                let raw_hex = to_hex(&hvcc_box.payload);

                video_profile = Some(VideoProfile {
                    codec: "hvc1".to_string(),
                    width,
                    height,
                    timescale,
                    nal_length_size: nal_len,
                    avc: None,
                    hevc: hevc_cfg,
                    raw_config_hex: raw_hex,
                });
            } else if is_video && video_profile.is_none() {
                // Fallback for raw video track
                video_profile = Some(VideoProfile {
                    codec: "unknown".to_string(),
                    width,
                    height,
                    timescale,
                    nal_length_size: 4,
                    avc: None,
                    hevc: None,
                    raw_config_hex: String::new(),
                });
            }

            // Audio track parsing
            if let Some(esds_box) = trak.find_descendant(b"esds") {
                let aac_cfg = AacConfig::parse_esds(&esds_box.payload);
                let sample_rate = aac_cfg.as_ref().map(|c| c.sample_rate).unwrap_or(44100);
                let channels = aac_cfg.as_ref().map(|c| c.channel_config as u16).unwrap_or(2);
                let raw_hex = to_hex(&esds_box.payload);

                audio_profile = Some(AudioProfile {
                    codec: "mp4a".to_string(),
                    sample_rate,
                    channels,
                    timescale,
                    aac: aac_cfg,
                    raw_config_hex: raw_hex,
                });
            } else if is_audio && audio_profile.is_none() {
                // Check sample entry name (mp4a, twos, sowt, lpcm, etc.)
                let mut codec = "mp4a".to_string();
                let mut sample_rate = 44100u32;
                let mut channels = 2u16;

                if let Some(mp4a) = trak.find_descendant(b"mp4a") {
                    codec = "mp4a".to_string();
                    if mp4a.payload.len() >= 28 {
                        channels = u16::from_be_bytes([mp4a.payload[16], mp4a.payload[17]]);
                        let sr_fixed = u32::from_be_bytes([
                            mp4a.payload[22],
                            mp4a.payload[23],
                            mp4a.payload[24],
                            mp4a.payload[25],
                        ]);
                        sample_rate = sr_fixed >> 16;
                    }
                }

                audio_profile = Some(AudioProfile {
                    codec,
                    sample_rate,
                    channels,
                    timescale,
                    aac: Some(AacConfig::default()),
                    raw_config_hex: String::new(),
                });
            }
        }

        Ok(Self {
            source_filename: filename,
            file_size,
            major_brand,
            video: video_profile,
            audio: audio_profile,
        })
    }

    /// Save profile in clean human-readable TOML format
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let mut file = File::create(path)?;

        writeln!(file, "# Dredge Camera & Media Profile")?;
        writeln!(file, "# Generated by dredge 0.1.0")?;
        writeln!(file)?;
        writeln!(file, "[source]")?;
        writeln!(file, "filename = \"{}\"", self.source_filename)?;
        writeln!(file, "size_bytes = {}", self.file_size)?;
        writeln!(file, "major_brand = \"{}\"", self.major_brand)?;
        writeln!(file)?;

        if let Some(v) = &self.video {
            writeln!(file, "[video]")?;
            writeln!(file, "codec = \"{}\"", v.codec)?;
            writeln!(file, "width = {}", v.width)?;
            writeln!(file, "height = {}", v.height)?;
            writeln!(file, "timescale = {}", v.timescale)?;
            writeln!(file, "nal_length_size = {}", v.nal_length_size)?;
            if !v.raw_config_hex.is_empty() {
                writeln!(file, "config_hex = \"{}\"", v.raw_config_hex)?;
            }

            if let Some(avc) = &v.avc {
                writeln!(file, "profile_idc = {}", avc.profile_idc)?;
                writeln!(file, "level_idc = {}", avc.level_idc)?;
                for (i, sps) in avc.sps_list.iter().enumerate() {
                    writeln!(file, "sps_{} = \"{}\"", i, to_hex(sps))?;
                }
                for (i, pps) in avc.pps_list.iter().enumerate() {
                    writeln!(file, "pps_{} = \"{}\"", i, to_hex(pps))?;
                }
            }

            if let Some(hevc) = &v.hevc {
                writeln!(file, "general_profile_idc = {}", hevc.general_profile_idc)?;
                writeln!(file, "general_level_idc = {}", hevc.general_level_idc)?;
                for (i, vps) in hevc.vps_list.iter().enumerate() {
                    writeln!(file, "vps_{} = \"{}\"", i, to_hex(vps))?;
                }
                for (i, sps) in hevc.sps_list.iter().enumerate() {
                    writeln!(file, "sps_{} = \"{}\"", i, to_hex(sps))?;
                }
                for (i, pps) in hevc.pps_list.iter().enumerate() {
                    writeln!(file, "pps_{} = \"{}\"", i, to_hex(pps))?;
                }
            }
            writeln!(file)?;
        }

        if let Some(a) = &self.audio {
            writeln!(file, "[audio]")?;
            writeln!(file, "codec = \"{}\"", a.codec)?;
            writeln!(file, "sample_rate = {}", a.sample_rate)?;
            writeln!(file, "channels = {}", a.channels)?;
            writeln!(file, "timescale = {}", a.timescale)?;
            if !a.raw_config_hex.is_empty() {
                writeln!(file, "config_hex = \"{}\"", a.raw_config_hex)?;
            }
            if let Some(aac) = &a.aac {
                writeln!(file, "audio_object_type = {}", aac.audio_object_type)?;
                writeln!(file, "sample_rate_index = {}", aac.sample_rate_index)?;
                writeln!(file, "channel_config = {}", aac.channel_config)?;
            }
            writeln!(file)?;
        }

        Ok(())
    }

    /// Load profile from TOML format file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut content = String::new();
        File::open(path)?.read_to_string(&mut content)?;

        let mut source_filename = "reference.mp4".to_string();
        let mut file_size = 0u64;
        let mut major_brand = "isom".to_string();

        let mut current_section = "";

        // Video fields
        let mut v_codec = String::new();
        let mut v_width = 0u32;
        let mut v_height = 0u32;
        let mut v_timescale = 30000u32;
        let mut v_nal_len = 4usize;
        let mut v_config_hex = String::new();
        let mut v_profile_idc = 0u8;
        let mut v_level_idc = 0u8;
        let mut v_sps = Vec::new();
        let mut v_pps = Vec::new();
        let mut v_vps = Vec::new();
        let mut has_video = false;

        // Audio fields
        let mut a_codec = String::new();
        let mut a_sample_rate = 44100u32;
        let mut a_channels = 2u16;
        let mut a_timescale = 44100u32;
        let mut a_config_hex = String::new();
        let mut a_aot = 2u8;
        let mut a_sr_idx = 4u8;
        let mut a_ch_cfg = 2u8;
        let mut has_audio = false;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if trimmed == "[source]" {
                current_section = "source";
                continue;
            } else if trimmed == "[video]" {
                current_section = "video";
                has_video = true;
                continue;
            } else if trimmed == "[audio]" {
                current_section = "audio";
                has_audio = true;
                continue;
            }

            if let Some((k, v)) = trimmed.split_once('=') {
                let key = k.trim();
                let val = v.trim().trim_matches('"');

                match (current_section, key) {
                    ("source", "filename") => source_filename = val.to_string(),
                    ("source", "size_bytes") => file_size = val.parse().unwrap_or(0),
                    ("source", "major_brand") => major_brand = val.to_string(),

                    ("video", "codec") => v_codec = val.to_string(),
                    ("video", "width") => v_width = val.parse().unwrap_or(0),
                    ("video", "height") => v_height = val.parse().unwrap_or(0),
                    ("video", "timescale") => v_timescale = val.parse().unwrap_or(30000),
                    ("video", "nal_length_size") => v_nal_len = val.parse().unwrap_or(4),
                    ("video", "config_hex") => v_config_hex = val.to_string(),
                    ("video", "profile_idc") | ("video", "general_profile_idc") => {
                        v_profile_idc = val.parse().unwrap_or(0)
                    }
                    ("video", "level_idc") | ("video", "general_level_idc") => {
                        v_level_idc = val.parse().unwrap_or(0)
                    }
                    ("video", k) if k.starts_with("sps_") => {
                        if let Some(bytes) = from_hex(val) {
                            v_sps.push(bytes);
                        }
                    }
                    ("video", k) if k.starts_with("pps_") => {
                        if let Some(bytes) = from_hex(val) {
                            v_pps.push(bytes);
                        }
                    }
                    ("video", k) if k.starts_with("vps_") => {
                        if let Some(bytes) = from_hex(val) {
                            v_vps.push(bytes);
                        }
                    }

                    ("audio", "codec") => a_codec = val.to_string(),
                    ("audio", "sample_rate") => a_sample_rate = val.parse().unwrap_or(44100),
                    ("audio", "channels") => a_channels = val.parse().unwrap_or(2),
                    ("audio", "timescale") => a_timescale = val.parse().unwrap_or(44100),
                    ("audio", "config_hex") => a_config_hex = val.to_string(),
                    ("audio", "audio_object_type") => a_aot = val.parse().unwrap_or(2),
                    ("audio", "sample_rate_index") => a_sr_idx = val.parse().unwrap_or(4),
                    ("audio", "channel_config") => a_ch_cfg = val.parse().unwrap_or(2),
                    _ => {}
                }
            }
        }

        let video = if has_video {
            let avc = if v_codec == "avc1" || !v_sps.is_empty() {
                Some(AvcConfig {
                    profile_idc: v_profile_idc,
                    profile_compatibility: 0,
                    level_idc: v_level_idc,
                    nal_length_size: v_nal_len,
                    sps_list: v_sps.clone(),
                    pps_list: v_pps.clone(),
                })
            } else {
                None
            };

            let hevc = if v_codec == "hvc1" || !v_vps.is_empty() {
                Some(HevcConfig {
                    general_profile_idc: v_profile_idc,
                    general_level_idc: v_level_idc,
                    nal_length_size: v_nal_len,
                    vps_list: v_vps,
                    sps_list: v_sps,
                    pps_list: v_pps,
                })
            } else {
                None
            };

            Some(VideoProfile {
                codec: v_codec,
                width: v_width,
                height: v_height,
                timescale: v_timescale,
                nal_length_size: v_nal_len,
                avc,
                hevc,
                raw_config_hex: v_config_hex,
            })
        } else {
            None
        };

        let audio = if has_audio {
            let aac = if a_codec == "mp4a" || !a_config_hex.is_empty() {
                Some(AacConfig {
                    audio_object_type: a_aot,
                    sample_rate_index: a_sr_idx,
                    channel_config: a_ch_cfg,
                    sample_rate: a_sample_rate,
                })
            } else {
                None
            };

            Some(AudioProfile {
                codec: a_codec,
                sample_rate: a_sample_rate,
                channels: a_channels,
                timescale: a_timescale,
                aac,
                raw_config_hex: a_config_hex,
            })
        } else {
            None
        };

        Ok(Self {
            source_filename,
            file_size,
            major_brand,
            video,
            audio,
        })
    }
}

pub fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut s, "{:02x}", b);
    }
    s
}

pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}
