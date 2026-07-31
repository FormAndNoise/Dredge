use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process;

const VIDEO_HDR: &str = "video.hdr";
const AUDIO_HDR: &str = "audio.hdr";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Analyze,
    Recover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VideoOutKind {
    H264,
    Hevc,
    NoVideo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioOutKind {
    Aac,
    Wav,
    Mp3,
    Raw,
    NoAudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Template {
    GoPro4,
    Ambarella,
    Lav,
    MainConcept,
    Novatek,
    Qt,
    Iphone4,
    Eos,
    Eos3,
    Eos4,
    Canon,
    Wowza,
    Vmix,
    Blackmagic,
    Avermedia,
    Lumia,
    Panasonic,
    Sony,
    Drim,
    Drim5,
    Tomtom,
    Icat,
    Cgo3,
    DjiAvc,
    DjiHvc,
    Hevc,
    Ext,
}

#[derive(Debug, Default, Clone)]
struct RecoverOptions {
    start: Option<u64>,
    end: Option<u64>,

    avc_idr_max: Option<u64>,
    avc_x_max: Option<u64>,
    aac_min: Option<u64>,
    pcm_min: Option<u64>,
    pcm_fix: Option<u64>,

    aac_long: bool,
    verbose: bool,

    template: Option<Template>,
}

#[derive(Debug)]
struct AnalyzeCommand {
    input: PathBuf,
}

#[derive(Debug)]
struct RecoverCommand {
    input: PathBuf,
    video_out: Option<PathBuf>,
    audio_out: Option<PathBuf>,
    video_kind: VideoOutKind,
    audio_kind: AudioOutKind,
    options: RecoverOptions,
}

#[derive(Debug)]
enum Command {
    Analyze(AnalyzeCommand),
    Recover(RecoverCommand),
}

fn main() {
    let args: Vec<OsString> = env::args_os().skip(1).collect();

    match parse_args(&args) {
        Ok(command) => {
            let result = match command {
                Command::Analyze(cmd) => analyze(cmd),
                Command::Recover(cmd) => recover(cmd),
            };

            if let Err(err) = result {
                println!("Error: {}", err);
                process::exit(1);
            }

            process::exit(0);
        }
        Err(ParseFailure::Help) => {
            print_help();
            process::exit(1);
        }
        Err(ParseFailure::Error(msg)) => {
            println!("Error: {}", msg);
            print_help();
            process::exit(1);
        }
    }
}

#[derive(Debug)]
enum ParseFailure {
    Help,
    Error(String),
}

fn parse_args(args: &[OsString]) -> Result<Command, ParseFailure> {
    if args.is_empty() {
        return Err(ParseFailure::Help);
    }

    if args.len() == 1 {
        if arg_eq(&args[0], "--help") || arg_eq(&args[0], "-h") {
            return Err(ParseFailure::Help);
        }
    }

    for arg in args {
        if arg_eq(arg, "--help") || arg_eq(arg, "-h") {
            return Err(ParseFailure::Help);
        }
    }

    // Analyze mode:
    //
    // recover_mp4 <in_good_similar.mp4> --analyze
    if args.len() == 2 && arg_eq(&args[1], "--analyze") {
        return Ok(Command::Analyze(AnalyzeCommand {
            input: PathBuf::from(&args[0]),
        }));
    }

    // Recover mode:
    //
    // recover_mp4 <in_corrupted.mp4> <out_video> <out_audio> [options]
    if args.len() >= 3 {
        let input = PathBuf::from(&args[0]);

        let video_token = args[1].to_string_lossy().to_string();
        let audio_token = args[2].to_string_lossy().to_string();

        let video_kind = parse_video_out_kind(&video_token)
            .ok_or_else(|| ParseFailure::Error(format!("invalid video output: {}", video_token)))?;

        let audio_kind = parse_audio_out_kind(&audio_token)
            .ok_or_else(|| ParseFailure::Error(format!("invalid audio output: {}", audio_token)))?;

        let video_out = if video_kind == VideoOutKind::NoVideo {
            None
        } else {
            Some(PathBuf::from(&args[1]))
        };

        let audio_out = if audio_kind == AudioOutKind::NoAudio {
            None
        } else {
            Some(PathBuf::from(&args[2]))
        };

        let options = parse_recover_options(&args[3..])?;

        return Ok(Command::Recover(RecoverCommand {
            input,
            video_out,
            audio_out,
            video_kind,
            audio_kind,
            options,
        }));
    }

    Err(ParseFailure::Help)
}

fn parse_recover_options(args: &[OsString]) -> Result<RecoverOptions, ParseFailure> {
    let mut options = RecoverOptions::default();

    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i].to_string_lossy();

        match arg.as_ref() {
            "--start" => {
                i += 1;
                options.start = Some(parse_required_hex(args, i, "--start")?);
            }
            "--end" => {
                i += 1;
                options.end = Some(parse_required_hex(args, i, "--end")?);
            }
            "--avcidrmax" => {
                i += 1;
                options.avc_idr_max = Some(parse_required_hex(args, i, "--avcidrmax")?);
            }
            "--avcxmax" => {
                i += 1;
                options.avc_x_max = Some(parse_required_hex(args, i, "--avcxmax")?);
            }
            "--aacmin" => {
                i += 1;
                options.aac_min = Some(parse_required_hex(args, i, "--aacmin")?);
            }
            "--pcmmin" => {
                i += 1;
                options.pcm_min = Some(parse_required_hex(args, i, "--pcmmin")?);
            }
            "--pcmfix" => {
                i += 1;
                options.pcm_fix = Some(parse_required_hex(args, i, "--pcmfix")?);
            }

            "--aaclong" => {
                options.aac_long = true;
            }
            "--verbose" => {
                options.verbose = true;
            }

            "--gopro4" => options.template = Some(Template::GoPro4),
            "--ambarella" => options.template = Some(Template::Ambarella),
            "--lav" => options.template = Some(Template::Lav),
            "--mainconcept" => options.template = Some(Template::MainConcept),
            "--novatek" => options.template = Some(Template::Novatek),
            "--qt" => options.template = Some(Template::Qt),
            "--iphone4" => options.template = Some(Template::Iphone4),
            "--eos" => options.template = Some(Template::Eos),
            "--eos3" => options.template = Some(Template::Eos3),
            "--eos4" => options.template = Some(Template::Eos4),
            "--canon" => options.template = Some(Template::Canon),
            "--wowza" => options.template = Some(Template::Wowza),
            "--vmix" => options.template = Some(Template::Vmix),
            "--blackmagic" => options.template = Some(Template::Blackmagic),
            "--avermedia" => options.template = Some(Template::Avermedia),
            "--lumia" => options.template = Some(Template::Lumia),
            "--panasonic" => options.template = Some(Template::Panasonic),
            "--sony" => options.template = Some(Template::Sony),
            "--drim" => options.template = Some(Template::Drim),
            "--drim5" => options.template = Some(Template::Drim5),
            "--tomtom" => options.template = Some(Template::Tomtom),
            "--icat" => options.template = Some(Template::Icat),
            "--cgo3" => options.template = Some(Template::Cgo3),
            "--dji.avc" => options.template = Some(Template::DjiAvc),
            "--dji.hvc" => options.template = Some(Template::DjiHvc),
            "--hevc" => options.template = Some(Template::Hevc),
            "--ext" => options.template = Some(Template::Ext),

            other => {
                return Err(ParseFailure::Error(format!("unknown option: {}", other)));
            }
        }

        i += 1;
    }

    Ok(options)
}

fn parse_required_hex(args: &[OsString], idx: usize, flag: &str) -> Result<u64, ParseFailure> {
    if idx >= args.len() {
        return Err(ParseFailure::Error(format!("missing value for {}", flag)));
    }

    let s = args[idx].to_string_lossy();
    parse_hex_value(&s)
        .map_err(|_| ParseFailure::Error(format!("invalid hexadecimal value for {}: {}", flag, s)))
}

fn parse_hex_value(s: &str) -> Result<u64, std::num::ParseIntError> {
    if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(rest, 16)
    } else {
        // The legacy tool accepts hex-like values. Treat unprefixed values as base 16.
        u64::from_str_radix(s, 16)
    }
}

fn parse_video_out_kind(s: &str) -> Option<VideoOutKind> {
    match s {
        "--novideo" => Some(VideoOutKind::NoVideo),
        _ if s.ends_with(".h264") => Some(VideoOutKind::H264),
        _ if s.ends_with(".hevc") => Some(VideoOutKind::Hevc),
        _ => None,
    }
}

fn parse_audio_out_kind(s: &str) -> Option<AudioOutKind> {
    match s {
        "--noaudio" => Some(AudioOutKind::NoAudio),
        _ if s.ends_with(".aac") => Some(AudioOutKind::Aac),
        _ if s.ends_with(".wav") => Some(AudioOutKind::Wav),
        _ if s.ends_with(".mp3") => Some(AudioOutKind::Mp3),
        _ if s.ends_with(".raw") => Some(AudioOutKind::Raw),
        _ => None,
    }
}

fn arg_eq(arg: &OsString, expected: &str) -> bool {
    arg.to_string_lossy() == expected
}

fn analyze(cmd: AnalyzeCommand) -> io::Result<()> {
    println!("recover_mp4 v1.92");
    println!("Analyze phase");
    println!("Input file: {}", cmd.input.display());

    let mut file = File::open(&cmd.input)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    let analysis = analyze_mp4_like_file(&data);

    write_video_hdr(&analysis)?;
    write_audio_hdr(&analysis)?;

    println!();
    println!("Created {}", VIDEO_HDR);
    println!("Created {}", AUDIO_HDR);
    println!();
    println!("To recover a damaged file, run:");
    println!(
        "recover_mp4 <in_corrupted.mp4> out_video.h264 out_audio.aac"
    );
    println!();
    println!("Then mux with ffmpeg, for example:");
    println!(
        "ffmpeg -r <fps> -i out_video.h264 -i out_audio.aac -c copy recovered.mp4"
    );
    println!();
    println!("For Yamb/My MP4Box workflows, import the recovered elementary streams manually.");

    Ok(())
}

#[derive(Debug, Default)]
struct FileAnalysis {
    has_ftyp: bool,
    has_moov: bool,
    has_mdat: bool,
    video_codec: Option<String>,
    audio_codec: Option<String>,
    avcc: Option<Vec<u8>>,
    hvcc: Option<Vec<u8>>,
    esds: Option<Vec<u8>>,
}

fn analyze_mp4_like_file(data: &[u8]) -> FileAnalysis {
    let mut analysis = FileAnalysis::default();

    analysis.has_ftyp = find_ascii(data, b"ftyp").is_some();
    analysis.has_moov = find_ascii(data, b"moov").is_some();
    analysis.has_mdat = find_ascii(data, b"mdat").is_some();

    if find_ascii(data, b"avc1").is_some() || find_ascii(data, b"avcC").is_some() {
        analysis.video_codec = Some("avc/h264".to_string());
    } else if find_ascii(data, b"hvc1").is_some()
        || find_ascii(data, b"hev1").is_some()
        || find_ascii(data, b"hvcC").is_some()
    {
        analysis.video_codec = Some("hevc/h265".to_string());
    }

    if find_ascii(data, b"mp4a").is_some() || find_ascii(data, b"esds").is_some() {
        analysis.audio_codec = Some("aac".to_string());
    } else if find_ascii(data, b"mp3").is_some() || find_ascii(data, b".mp3").is_some() {
        analysis.audio_codec = Some("mp3".to_string());
    } else if find_ascii(data, b"twos").is_some()
        || find_ascii(data, b"sowt").is_some()
        || find_ascii(data, b"lpcm").is_some()
    {
        analysis.audio_codec = Some("pcm".to_string());
    }

    analysis.avcc = extract_box_payload_by_type(data, b"avcC");
    analysis.hvcc = extract_box_payload_by_type(data, b"hvcC");
    analysis.esds = extract_box_payload_by_type(data, b"esds");

    analysis
}

fn write_video_hdr(analysis: &FileAnalysis) -> io::Result<()> {
    let mut file = File::create(VIDEO_HDR)?;

    writeln!(file, "recover_mp4 rust cleanroom header")?;
    writeln!(file, "type=video")?;
    writeln!(
        file,
        "codec={}",
        analysis.video_codec.as_deref().unwrap_or("unknown")
    )?;
    writeln!(file, "has_ftyp={}", analysis.has_ftyp)?;
    writeln!(file, "has_moov={}", analysis.has_moov)?;
    writeln!(file, "has_mdat={}", analysis.has_mdat)?;

    if let Some(avcc) = &analysis.avcc {
        writeln!(file, "box=avcC")?;
        writeln!(file, "payload_hex={}", to_hex(avcc))?;
    }

    if let Some(hvcc) = &analysis.hvcc {
        writeln!(file, "box=hvcC")?;
        writeln!(file, "payload_hex={}", to_hex(hvcc))?;
    }

    Ok(())
}

fn write_audio_hdr(analysis: &FileAnalysis) -> io::Result<()> {
    let mut file = File::create(AUDIO_HDR)?;

    writeln!(file, "recover_mp4 rust cleanroom header")?;
    writeln!(file, "type=audio")?;
    writeln!(
        file,
        "codec={}",
        analysis.audio_codec.as_deref().unwrap_or("unknown")
    )?;
    writeln!(file, "has_ftyp={}", analysis.has_ftyp)?;
    writeln!(file, "has_moov={}", analysis.has_moov)?;
    writeln!(file, "has_mdat={}", analysis.has_mdat)?;

    if let Some(esds) = &analysis.esds {
        writeln!(file, "box=esds")?;
        writeln!(file, "payload_hex={}", to_hex(esds))?;
    }

    Ok(())
}

fn recover(cmd: RecoverCommand) -> io::Result<()> {
    println!("recover_mp4 v1.92");
    println!("Recover phase");
    println!("Input file: {}", cmd.input.display());

    require_hdr_files()?;

    let video_hdr = fs::read(VIDEO_HDR)?;
    let audio_hdr = fs::read(AUDIO_HDR)?;

    if cmd.options.verbose {
        println!("Loaded {}: {} bytes", VIDEO_HDR, video_hdr.len());
        println!("Loaded {}: {} bytes", AUDIO_HDR, audio_hdr.len());
        println!("Options: {:?}", cmd.options);
    }

    let mut input_file = File::open(&cmd.input)?;
    let mut input = Vec::new();
    input_file.read_to_end(&mut input)?;

    let file_len = input.len() as u64;
    let start = cmd.options.start.unwrap_or_else(|| find_mdat_payload_start(&input).unwrap_or(0));
    let mut end = cmd.options.end.unwrap_or(file_len);

    if end == 0 {
        end = file_len;
    }

    if start > file_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--start is beyond end of input file",
        ));
    }

    if end > file_len {
        end = file_len;
    }

    if end < start {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--end is before --start",
        ));
    }

    let region = &input[start as usize..end as usize];

    if cmd.options.verbose {
        println!("Read region: 0x{:x}..0x{:x}", start, end);
        println!("Region size: {} bytes", region.len());
    }

    if let Some(video_out) = &cmd.video_out {
        match cmd.video_kind {
            VideoOutKind::H264 => {
                let bytes = recover_h264_like_stream(region, &cmd.options);
                fs::write(video_out, bytes)?;
                println!("Wrote video stream: {}", video_out.display());
            }
            VideoOutKind::Hevc => {
                let bytes = recover_hevc_like_stream(region, &cmd.options);
                fs::write(video_out, bytes)?;
                println!("Wrote video stream: {}", video_out.display());
            }
            VideoOutKind::NoVideo => {}
        }
    } else {
        println!("Video disabled");
    }

    if let Some(audio_out) = &cmd.audio_out {
        match cmd.audio_kind {
            AudioOutKind::Aac => {
                let bytes = recover_aac_like_stream(region, &cmd.options);
                fs::write(audio_out, bytes)?;
                println!("Wrote audio stream: {}", audio_out.display());
            }
            AudioOutKind::Wav => {
                let bytes = recover_wav_like_stream(region, &cmd.options);
                fs::write(audio_out, bytes)?;
                println!("Wrote audio stream: {}", audio_out.display());
            }
            AudioOutKind::Mp3 => {
                let bytes = recover_mp3_like_stream(region, &cmd.options);
                fs::write(audio_out, bytes)?;
                println!("Wrote audio stream: {}", audio_out.display());
            }
            AudioOutKind::Raw => {
                let bytes = recover_raw_audio_stream(region, &cmd.options);
                fs::write(audio_out, bytes)?;
                println!("Wrote audio stream: {}", audio_out.display());
            }
            AudioOutKind::NoAudio => {}
        }
    } else {
        println!("Audio disabled");
    }

    Ok(())
}

fn require_hdr_files() -> io::Result<()> {
    if !Path::new(VIDEO_HDR).exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "video.hdr is missing in current working directory",
        ));
    }

    if !Path::new(AUDIO_HDR).exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "audio.hdr is missing in current working directory",
        ));
    }

    Ok(())
}

fn recover_h264_like_stream(region: &[u8], options: &RecoverOptions) -> Vec<u8> {
    // Conservative elementary-stream extraction:
    //
    // 1. Preserve data if Annex B start codes already appear.
    // 2. Otherwise attempt length-prefixed AVC NAL extraction using 4-byte sizes.
    //
    // The legacy program contains device-specific heuristics not described in the
    // provided specification. This function is intentionally isolated so that the
    // exact heuristics can be implemented without changing the CLI layer.

    if contains_annex_b_start_code(region) {
        return extract_annex_b_like(region);
    }

    extract_length_prefixed_nals(region, CodecKind::H264, options)
}

fn recover_hevc_like_stream(region: &[u8], options: &RecoverOptions) -> Vec<u8> {
    if contains_annex_b_start_code(region) {
        return extract_annex_b_like(region);
    }

    extract_length_prefixed_nals(region, CodecKind::Hevc, options)
}

fn recover_aac_like_stream(region: &[u8], _options: &RecoverOptions) -> Vec<u8> {
    // Try to preserve ADTS AAC frames.
    let mut out = Vec::new();
    let mut i = 0usize;

    while i + 7 <= region.len() {
        if region[i] == 0xff && (region[i + 1] & 0xf0) == 0xf0 {
            let frame_len = (((region[i + 3] & 0x03) as usize) << 11)
                | ((region[i + 4] as usize) << 3)
                | (((region[i + 5] & 0xe0) as usize) >> 5);

            if frame_len >= 7 && i + frame_len <= region.len() {
                out.extend_from_slice(&region[i..i + frame_len]);
                i += frame_len;
                continue;
            }
        }

        i += 1;
    }

    out
}

fn recover_wav_like_stream(region: &[u8], _options: &RecoverOptions) -> Vec<u8> {
    // Without exact PCM parameters from the legacy hdr format, emit a minimal
    // placeholder WAV with region data as 16-bit LE stereo at 48 kHz.
    //
    // This keeps the output type valid while allowing later replacement with
    // exact header-derived behavior.
    make_wav(region, 48_000, 2, 16)
}

fn recover_mp3_like_stream(region: &[u8], _options: &RecoverOptions) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0usize;

    while i + 4 <= region.len() {
        if region[i] == 0xff && (region[i + 1] & 0xe0) == 0xe0 {
            // MP3 frame size calculation is intentionally conservative here.
            // If sync is detected, copy a typical maximum chunk until next sync
            // or end.
            let next = find_next_mp3_sync(region, i + 1).unwrap_or(region.len());
            out.extend_from_slice(&region[i..next]);
            i = next;
        } else {
            i += 1;
        }
    }

    out
}

fn recover_raw_audio_stream(region: &[u8], _options: &RecoverOptions) -> Vec<u8> {
    region.to_vec()
}

#[derive(Debug, Clone, Copy)]
enum CodecKind {
    H264,
    Hevc,
}

fn extract_length_prefixed_nals(
    region: &[u8],
    codec: CodecKind,
    options: &RecoverOptions,
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0usize;

    while i + 4 <= region.len() {
        // --- CAMERA HEURISTICS ---
        if let Some(Template::GoPro4) = options.template {
            let tag = &region[i..i + 4];
            // Identify GoPro GPMF metadata chunks (DEVC, DVID, STRM) and skip them
            if tag == b"DEVC" || tag == b"DVID" || tag == b"STRM" || tag == b"RMRK" || tag == b"TYPE" {
                if i + 8 <= region.len() {
                    let word1 = u32::from_be_bytes([region[i+4], region[i+5], region[i+6], region[i+7]]);
                    let chunk_len = (word1 & 0x0000_ffff) as usize + 8;
                    i += chunk_len;
                    continue;
                }
            }
            // Identify GoPro FDSC recovery blocks ("GP")
            if tag[0] == b'G' && tag[1] == b'P' {
                // Approximate fdsc skip (usually 16 or 152 bytes, we'll try a minimal skip)
                i += 16;
                continue;
            }
        }
        // -------------------------

        // --- GENERIC METADATA HEURISTICS ---
        // TMCD: Timecode tracks (often 4 bytes payload)
        if region.len() >= i + 8 {
            let tag = &region[i + 4..i + 8];
            if tag == b"tmcd" {
                i += 12; // 4 (size) + 4 (tag) + 4 (payload)
                continue;
            }
        }
        
        // CAMM: Camera Metadata (Google Street View, Insta360, etc.)
        if region.len() >= i + 8 {
            let tag = &region[i + 4..i + 8];
            if tag == b"camm" {
                if i + 10 <= region.len() {
                    let type_val = (region[i + 8] as u16) | ((region[i + 9] as u16) << 8);
                    let lengths = [4, 12, 16, 28, 4, 4, 8, 4];
                    if (type_val as usize) < lengths.len() {
                        i += lengths[type_val as usize] + 8;
                        continue;
                    }
                }
            }
        }
        // -------------------------

        let size = u32::from_be_bytes([region[i], region[i + 1], region[i + 2], region[i + 3]])
            as usize;

        if size == 0 || i + 4 + size > region.len() {
            i += 1;
            continue;
        }

        let nal = &region[i + 4..i + 4 + size];

        if is_plausible_nal(nal, codec, options) {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(nal);
            i += 4 + size;
        } else {
            i += 1;
        }
    }

    out
}

fn is_plausible_nal(nal: &[u8], codec: CodecKind, options: &RecoverOptions) -> bool {
    if nal.is_empty() {
        return false;
    }

    match codec {
        CodecKind::H264 => {
            let nal_type = nal[0] & 0x1f;

            if nal_type == 0 || nal_type > 23 {
                return false;
            }

            if nal_type == 5 {
                if let Some(max) = options.avc_idr_max {
                    if nal.len() as u64 > max {
                        return false;
                    }
                }
            } else if let Some(max) = options.avc_x_max {
                if nal.len() as u64 > max {
                    return false;
                }
            }

            true
        }
        CodecKind::Hevc => {
            if nal.len() < 2 {
                return false;
            }

            let nal_type = (nal[0] >> 1) & 0x3f;
            nal_type <= 40
        }
    }
}

fn contains_annex_b_start_code(data: &[u8]) -> bool {
    data.windows(3).any(|w| w == [0, 0, 1])
        || data.windows(4).any(|w| w == [0, 0, 0, 1])
}

fn extract_annex_b_like(data: &[u8]) -> Vec<u8> {
    let first_3 = find_bytes(data, &[0, 0, 1]);
    let first_4 = find_bytes(data, &[0, 0, 0, 1]);

    let start = match (first_3, first_4) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => 0,
    };

    data[start..].to_vec()
}

fn find_next_mp3_sync(data: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < data.len() {
        if data[i] == 0xff && (data[i + 1] & 0xe0) == 0xe0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn make_wav(pcm: &[u8], sample_rate: u32, channels: u16, bits_per_sample: u16) -> Vec<u8> {
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_len = pcm.len() as u32;
    let riff_len = 36u32.saturating_add(data_len);

    let mut out = Vec::with_capacity(44 + pcm.len());

    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_len.to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());

    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(pcm);

    out
}

fn find_mdat_payload_start(data: &[u8]) -> Option<u64> {
    let pos = find_ascii(data, b"mdat")?;

    if pos < 4 {
        return Some((pos + 4) as u64);
    }

    let size_start = pos - 4;
    let size = u32::from_be_bytes([
        data[size_start],
        data[size_start + 1],
        data[size_start + 2],
        data[size_start + 3],
    ]);

    if size == 1 {
        // 64-bit extended size: size32 + type + largesize
        Some((pos + 12) as u64)
    } else {
        Some((pos + 4) as u64)
    }
}

fn extract_box_payload_by_type(data: &[u8], box_type: &[u8; 4]) -> Option<Vec<u8>> {
    let mut i = 0usize;

    while i + 8 <= data.len() {
        let size = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        let typ = &data[i + 4..i + 8];

        if typ == box_type {
            if size >= 8 && i + size <= data.len() {
                return Some(data[i + 8..i + size].to_vec());
            }
        }

        if size >= 8 {
            i += size;
        } else {
            i += 1;
        }
    }

    // Fallback: search by type string and try to infer size from preceding 4 bytes.
    if let Some(pos) = find_ascii(data, box_type) {
        if pos >= 4 {
            let size_start = pos - 4;
            let size = u32::from_be_bytes([
                data[size_start],
                data[size_start + 1],
                data[size_start + 2],
                data[size_start + 3],
            ]) as usize;

            if size >= 8 && size_start + size <= data.len() {
                return Some(data[pos + 4..size_start + size].to_vec());
            }
        }
    }

    None
}

fn find_ascii(data: &[u8], needle: &[u8]) -> Option<usize> {
    find_bytes(data, needle)
}

fn find_bytes(data: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > data.len() {
        return None;
    }

    data.windows(needle.len()).position(|w| w == needle)
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut s, "{:02x}", b);
    }
    s
}

fn print_help() {
    println!("recover_mp4 v1.92");
    println!();
    println!("Usage:");
    println!("  recover_mp4 <in_good_similar.mp4> --analyze");
    println!("  recover_mp4 <in_corrupted.mp4> <out_video> <out_audio> [options]");
    println!();
    println!("Analyze mode:");
    println!("  Creates video.hdr and audio.hdr in the current working directory.");
    println!();
    println!("Recover mode:");
    println!("  <out_video> must be one of:");
    println!("    out_video.h264");
    println!("    out_video.hevc");
    println!("    --novideo");
    println!();
    println!("  <out_audio> must be one of:");
    println!("    out_audio.aac");
    println!("    out_audio.wav");
    println!("    out_audio.mp3");
    println!("    out_audio.raw");
    println!("    --noaudio");
    println!();
    println!("Options:");
    println!("  --start <hex>       Read from position, ignoring mdat atom");
    println!("  --end <hex>         End position, ignoring mdat atom; 0 means EOF");
    println!("  --avcidrmax <hex>   Ignore AVC IDR NAL units larger than this size");
    println!("  --avcxmax <hex>     Ignore AVC non-IDR NAL units larger than this size");
    println!("  --aacmin <hex>      Minimum AAC value");
    println!("  --pcmmin <hex>      Minimum PCM value");
    println!("  --pcmfix <hex>      PCM fix value");
    println!("  --aaclong           Enables long AAC processing");
    println!("  --verbose           Enables verbose diagnostic output");
    println!();
    println!("Device/template flags:");
    println!("  --gopro4");
    println!("  --ambarella");
    println!("  --lav");
    println!("  --mainconcept");
    println!("  --novatek");
    println!("  --qt");
    println!("  --iphone4");
    println!("  --eos");
    println!("  --eos3");
    println!("  --eos4");
    println!("  --canon");
    println!("  --wowza");
    println!("  --vmix");
    println!("  --blackmagic");
    println!("  --avermedia");
    println!("  --lumia");
    println!("  --panasonic");
    println!("  --sony");
    println!("  --drim");
    println!("  --drim5");
    println!("  --tomtom");
    println!("  --icat");
    println!("  --cgo3");
    println!("  --dji.avc");
    println!("  --dji.hvc");
    println!("  --hevc");
    println!("  --ext");
}