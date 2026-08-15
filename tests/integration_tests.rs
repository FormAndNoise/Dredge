use dredge::codecs::{AacConfig, AvcConfig, HevcConfig};
use dredge::mp4_box::parse_boxes;
use dredge::profile::{AudioProfile, DredgeProfile, VideoProfile};
use dredge::scanner::{self, AudioFormat, ScanOptions, VideoFormat};
use std::fs::File;
use std::io::Write;

fn make_box(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let size = (payload.len() + 8) as u32;
    let mut out = Vec::new();
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(box_type);
    out.extend_from_slice(payload);
    out
}

#[test]
fn test_recursive_box_parser() {
    // Construct nested hierarchy:
    // moov -> trak -> mdia -> minf -> stbl -> stsd -> avc1 (78 bytes fixed header) -> avcC
    let avcc_payload = vec![1, 0x64, 0x00, 0x1F, 0xFF, 0xE1, 0x00, 0x04, 0x67, 0x64, 0x00, 0x1F, 0x01, 0x00, 0x04, 0x68, 0xEE, 0x38, 0x80];
    let avcc_box = make_box(b"avcC", &avcc_payload);

    let mut avc1_payload = vec![0u8; 78]; // 78 bytes fixed visual sample entry header
    avc1_payload.extend_from_slice(&avcc_box);
    let avc1_box = make_box(b"avc1", &avc1_payload);

    let mut stsd_payload = vec![0u8; 8]; // 4 bytes version/flags + 4 bytes entry count
    stsd_payload.extend_from_slice(&avc1_box);
    let stsd_box = make_box(b"stsd", &stsd_payload);

    let stbl_box = make_box(b"stbl", &stsd_box);
    let minf_box = make_box(b"minf", &stbl_box);
    let mdia_box = make_box(b"mdia", &minf_box);
    let trak_box = make_box(b"trak", &mdia_box);
    let moov_box = make_box(b"moov", &trak_box);

    let boxes = parse_boxes(&moov_box, 0);
    assert_eq!(boxes.len(), 1);
    assert_eq!(&boxes[0].box_type, b"moov");

    let avcc_found = boxes[0].find_descendant(b"avcC");
    assert!(avcc_found.is_some(), "Recursive box parser must find avcC nested inside moov/trak/mdia/minf/stbl/stsd/avc1");
    let avcc_unwrapped = avcc_found.unwrap();
    assert_eq!(avcc_unwrapped.payload, avcc_payload);
}

#[test]
fn test_avc_config_and_annex_b() {
    let avcc_payload = vec![
        1, 0x64, 0x00, 0x1F, 0xFF,
        0xE1, // 1 SPS
        0x00, 0x04, 0x67, 0x64, 0x00, 0x1F, // SPS
        0x01, // 1 PPS
        0x00, 0x04, 0x68, 0xEE, 0x38, 0x80, // PPS
    ];

    let avc = AvcConfig::parse_avcc(&avcc_payload).expect("Failed to parse avcC");
    assert_eq!(avc.profile_idc, 0x64);
    assert_eq!(avc.level_idc, 0x1F);
    assert_eq!(avc.nal_length_size, 4);
    assert_eq!(avc.sps_list.len(), 1);
    assert_eq!(avc.pps_list.len(), 1);

    let annex_b = avc.to_annex_b_header();
    assert_eq!(annex_b.len(), 16);
    assert_eq!(&annex_b[0..4], &[0x00, 0x00, 0x00, 0x01]);
    assert_eq!(&annex_b[8..12], &[0x00, 0x00, 0x00, 0x01]);
}

#[test]
fn test_hevc_config_and_annex_b() {
    let mut hvcc_payload = vec![0u8; 23];
    hvcc_payload[0] = 1;      // version 1
    hvcc_payload[1] = 0x01;   // profile Main
    hvcc_payload[12] = 93;    // level 3.1
    hvcc_payload[21] = 0x03;  // NAL length size = 4
    hvcc_payload[22] = 2;     // 2 arrays

    // Array 1: VPS (type 32)
    hvcc_payload.push(0x80 | 32);
    hvcc_payload.extend_from_slice(&1u16.to_be_bytes()); // 1 NAL
    hvcc_payload.extend_from_slice(&4u16.to_be_bytes()); // len 4
    hvcc_payload.extend_from_slice(&[0x40, 0x01, 0x0C, 0x01]);

    // Array 2: SPS (type 33)
    hvcc_payload.push(0x80 | 33);
    hvcc_payload.extend_from_slice(&1u16.to_be_bytes());
    hvcc_payload.extend_from_slice(&4u16.to_be_bytes());
    hvcc_payload.extend_from_slice(&[0x42, 0x01, 0x01, 0x01]);

    let hevc = HevcConfig::parse_hvcc(&hvcc_payload).expect("Failed to parse hvcC");
    assert_eq!(hevc.general_profile_idc, 1);
    assert_eq!(hevc.general_level_idc, 93);
    assert_eq!(hevc.vps_list.len(), 1);
    assert_eq!(hevc.sps_list.len(), 1);

    let annex_b = hevc.to_annex_b_header();
    assert_eq!(annex_b.len(), 16);
    assert_eq!(&annex_b[0..4], &[0x00, 0x00, 0x00, 0x01]);
}

#[test]
fn test_aac_config_and_adts_synthesis() {
    let asc = vec![0x12, 0x10];
    let aac = AacConfig::from_audio_specific_config(&asc).expect("Failed to parse ASC");
    assert_eq!(aac.audio_object_type, 2);
    assert_eq!(aac.sample_rate_index, 4);
    assert_eq!(aac.channel_config, 2);
    assert_eq!(aac.sample_rate, 44100);

    let adts = aac.synthesize_adts_header(100);
    assert_eq!(adts[0], 0xFF);
    assert_eq!(adts[1] & 0xF0, 0xF0); // Sync word
    let total_len = (((adts[3] & 0x03) as usize) << 11) | ((adts[4] as usize) << 3) | ((adts[5] >> 5) as usize);
    assert_eq!(total_len, 107); // 100 + 7
}

#[test]
fn test_profile_roundtrip() {
    let profile = DredgeProfile {
        source_filename: "test_gopro.mp4".to_string(),
        file_size: 1048576,
        major_brand: "mp42".to_string(),
        video: Some(VideoProfile {
            codec: "avc1".to_string(),
            width: 3840,
            height: 2160,
            timescale: 60000,
            nal_length_size: 4,
            avc: Some(AvcConfig {
                profile_idc: 100,
                profile_compatibility: 0,
                level_idc: 51,
                nal_length_size: 4,
                sps_list: vec![vec![0x67, 0x64, 0x00, 0x33]],
                pps_list: vec![vec![0x68, 0xEE, 0x38, 0x80]],
            }),
            hevc: None,
            raw_config_hex: "01640033ffe100046764003301000468ee3880".to_string(),
        }),
        audio: Some(AudioProfile {
            codec: "mp4a".to_string(),
            sample_rate: 48000,
            channels: 2,
            timescale: 48000,
            aac: Some(AacConfig {
                audio_object_type: 2,
                sample_rate_index: 3,
                channel_config: 2,
                sample_rate: 48000,
            }),
            raw_config_hex: "1190".to_string(),
        }),
    };

    let temp_dir = std::env::temp_dir();
    let toml_path = temp_dir.join("dredge_test_profile.toml");
    profile.save_to_file(&toml_path).expect("Failed to save profile");

    let loaded = DredgeProfile::load_from_file(&toml_path).expect("Failed to load profile");
    assert_eq!(loaded.source_filename, "test_gopro.mp4");
    assert_eq!(loaded.file_size, 1048576);
    assert_eq!(loaded.major_brand, "mp42");

    let v = loaded.video.as_ref().unwrap();
    assert_eq!(v.width, 3840);
    assert_eq!(v.height, 2160);
    assert_eq!(v.timescale, 60000);
    assert_eq!(v.avc.as_ref().unwrap().sps_list.len(), 1);

    let a = loaded.audio.as_ref().unwrap();
    assert_eq!(a.sample_rate, 48000);
    assert_eq!(a.channels, 2);

    let _ = std::fs::remove_file(toml_path);
}

#[test]
fn test_end_to_end_video_and_audio_recovery() {
    let temp_dir = std::env::temp_dir();
    let damaged_mp4 = temp_dir.join("damaged_recording.mp4");
    let out_video = temp_dir.join("recovered_test.h264");
    let out_audio = temp_dir.join("recovered_test.aac");

    // Synthesize an unfinalized MP4 with an mdat atom containing length-prefixed NALs and ADTS audio frames
    let mut payload = Vec::new();
    
    // Video Frame 1: IDR frame (type 5) with 4-byte length prefix
    let nal1 = vec![0x65, 0x88, 0x80, 0x10, 0x20, 0x30];
    payload.extend_from_slice(&(nal1.len() as u32).to_be_bytes());
    payload.extend_from_slice(&nal1);

    // Video Frame 2: Non-IDR frame (type 1) with 4-byte length prefix
    let nal2 = vec![0x61, 0x9A, 0x20, 0x40];
    payload.extend_from_slice(&(nal2.len() as u32).to_be_bytes());
    payload.extend_from_slice(&nal2);

    // Audio Frame 1: ADTS frame
    let aac_cfg = AacConfig::default();
    let adts_hdr = aac_cfg.synthesize_adts_header(32);
    payload.extend_from_slice(&adts_hdr);
    payload.extend_from_slice(&vec![0xAA; 32]);

    let mdat_box = make_box(b"mdat", &payload);

    let mut file = File::create(&damaged_mp4).expect("create damaged mp4");
    file.write_all(&mdat_box).expect("write damaged mp4");
    drop(file);

    let profile = DredgeProfile {
        source_filename: "ref.mp4".to_string(),
        file_size: 1000,
        major_brand: "isom".to_string(),
        video: Some(VideoProfile {
            codec: "avc1".to_string(),
            width: 1920,
            height: 1080,
            timescale: 30000,
            nal_length_size: 4,
            avc: Some(AvcConfig {
                profile_idc: 100,
                profile_compatibility: 0,
                level_idc: 40,
                nal_length_size: 4,
                sps_list: vec![vec![0x67, 0x64, 0x00, 0x28]],
                pps_list: vec![vec![0x68, 0xEE, 0x38, 0x80]],
            }),
            hevc: None,
            raw_config_hex: String::new(),
        }),
        audio: Some(AudioProfile {
            codec: "mp4a".to_string(),
            sample_rate: 44100,
            channels: 2,
            timescale: 44100,
            aac: Some(aac_cfg),
            raw_config_hex: String::new(),
        }),
    };

    let opts = ScanOptions::default();

    // Recover Video
    let v_stats = scanner::recover_video(&damaged_mp4, &out_video, VideoFormat::H264, Some(&profile), &opts)
        .expect("recover video");
    assert!(v_stats.video_frames_recovered >= 2);
    assert!(v_stats.video_bytes_written > 0);

    // Read back recovered video and verify Annex B start codes
    let recovered_video_bytes = std::fs::read(&out_video).expect("read video");
    assert_eq!(&recovered_video_bytes[0..4], &[0x00, 0x00, 0x00, 0x01], "Must start with Annex B start code for SPS");

    // Recover Audio
    let a_stats = scanner::recover_audio(&damaged_mp4, &out_audio, AudioFormat::Aac, Some(&profile), &opts)
        .expect("recover audio");
    assert!(a_stats.audio_frames_recovered >= 1);

    // Cleanup
    let _ = std::fs::remove_file(damaged_mp4);
    let _ = std::fs::remove_file(out_video);
    let _ = std::fs::remove_file(out_audio);
}
