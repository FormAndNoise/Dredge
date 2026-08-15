use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process;

use dredge::profile::DredgeProfile;
use dredge::scanner::{self, AudioFormat, ScanOptions, VideoFormat};

const DEFAULT_PROFILE_NAME: &str = "dredge-profile.toml";

#[derive(Debug)]
struct AnalyzeCommand {
    working_input: PathBuf,
    profile_out: PathBuf,
    verbose: bool,
}

#[derive(Debug)]
struct RecoverCommand {
    broken_input: PathBuf,
    video_out: Option<PathBuf>,
    audio_out: Option<PathBuf>,
    video_format: Option<VideoFormat>,
    audio_format: Option<AudioFormat>,
    profile_path: PathBuf,
    options: ScanOptions,
}

#[derive(Debug)]
enum Command {
    Analyze(AnalyzeCommand),
    Recover(RecoverCommand),
    Help,
    Version,
}

fn main() {
    let args: Vec<OsString> = env::args_os().skip(1).collect();

    let command = match parse_cli_args(&args) {
        Ok(cmd) => cmd,
        Err(err) => {
            eprintln!("Error: {}", err);
            eprintln!("Run 'dredge --help' for usage information.");
            process::exit(1);
        }
    };

    match command {
        Command::Help => {
            print_help();
            process::exit(0);
        }
        Command::Version => {
            println!("dredge 0.1.0");
            process::exit(0);
        }
        Command::Analyze(cmd) => {
            if let Err(err) = run_analyze(cmd) {
                eprintln!("Analysis Error: {}", err);
                process::exit(1);
            }
        }
        Command::Recover(cmd) => {
            if let Err(err) = run_recover(cmd) {
                eprintln!("Recovery Error: {}", err);
                process::exit(1);
            }
        }
    }
}

fn parse_cli_args(args: &[OsString]) -> Result<Command, String> {
    if args.is_empty() {
        return Ok(Command::Help);
    }

    if args.len() == 1 {
        let first = args[0].to_string_lossy();
        if first == "--help" || first == "-h" || first == "help" {
            return Ok(Command::Help);
        }
        if first == "--version" || first == "-V" || first == "version" {
            return Ok(Command::Version);
        }
    }

    // Check for analyze mode: dredge <working.mp4> --analyze [--out <profile.toml>]
    let mut is_analyze = false;
    for arg in args {
        let s = arg.to_string_lossy();
        if s == "--analyze" {
            is_analyze = true;
            break;
        }
        if s == "--help" || s == "-h" {
            return Ok(Command::Help);
        }
        if s == "--version" || s == "-V" {
            return Ok(Command::Version);
        }
    }

    if is_analyze {
        let working_input = PathBuf::from(&args[0]);
        let mut profile_out = PathBuf::from(DEFAULT_PROFILE_NAME);
        let mut verbose = false;

        let mut i = 1usize;
        while i < args.len() {
            let s = args[i].to_string_lossy();
            if s == "--analyze" {
                i += 1;
            } else if s == "--out" || s == "-o" || s == "--profile" {
                if i + 1 >= args.len() {
                    return Err(format!("missing file path after {}", s));
                }
                profile_out = PathBuf::from(&args[i + 1]);
                i += 2;
            } else if s == "--verbose" || s == "-v" {
                verbose = true;
                i += 1;
            } else {
                i += 1;
            }
        }

        return Ok(Command::Analyze(AnalyzeCommand {
            working_input,
            profile_out,
            verbose,
        }));
    }

    // Recover mode: dredge <broken.mp4> <out_video> <out_audio> [OPTIONS]
    if args.len() >= 3 {
        let broken_input = PathBuf::from(&args[0]);

        let video_str = args[1].to_string_lossy();
        let audio_str = args[2].to_string_lossy();

        let (video_format, video_out) = parse_video_arg(&video_str)?;
        let (audio_format, audio_out) = parse_audio_arg(&audio_str)?;

        let mut profile_path = PathBuf::from(DEFAULT_PROFILE_NAME);
        let mut options = ScanOptions::default();

        let mut i = 3usize;
        while i < args.len() {
            let s = args[i].to_string_lossy();
            if s == "--profile" || s == "-p" {
                if i + 1 >= args.len() {
                    return Err("missing profile path after --profile".to_string());
                }
                profile_path = PathBuf::from(&args[i + 1]);
                i += 2;
            } else if s == "--start" {
                if i + 1 >= args.len() {
                    return Err("missing offset after --start".to_string());
                }
                options.start_offset = Some(parse_offset_str(&args[i + 1].to_string_lossy())?);
                i += 2;
            } else if s == "--end" {
                if i + 1 >= args.len() {
                    return Err("missing offset after --end".to_string());
                }
                options.end_offset = Some(parse_offset_str(&args[i + 1].to_string_lossy())?);
                i += 2;
            } else if s == "--verbose" || s == "-v" {
                options.verbose = true;
                i += 1;
            } else {
                // Ignore unrecognized or legacy flags gracefully with a warning
                i += 1;
            }
        }

        return Ok(Command::Recover(RecoverCommand {
            broken_input,
            video_out,
            audio_out,
            video_format,
            audio_format,
            profile_path,
            options,
        }));
    }

    Err("invalid command line arguments".to_string())
}

fn parse_video_arg(s: &str) -> Result<(Option<VideoFormat>, Option<PathBuf>), String> {
    if s == "--novideo" {
        Ok((None, None))
    } else if s.ends_with(".h264") || s.ends_with(".264") || s.ends_with(".avc") {
        Ok((Some(VideoFormat::H264), Some(PathBuf::from(s))))
    } else if s.ends_with(".hevc") || s.ends_with(".265") || s.ends_with(".h265") {
        Ok((Some(VideoFormat::Hevc), Some(PathBuf::from(s))))
    } else {
        Err(format!(
            "invalid video output '{}': must end in .h264, .hevc, or be --novideo",
            s
        ))
    }
}

fn parse_audio_arg(s: &str) -> Result<(Option<AudioFormat>, Option<PathBuf>), String> {
    if s == "--noaudio" {
        Ok((None, None))
    } else if s.ends_with(".aac") {
        Ok((Some(AudioFormat::Aac), Some(PathBuf::from(s))))
    } else if s.ends_with(".wav") {
        Ok((Some(AudioFormat::Wav), Some(PathBuf::from(s))))
    } else if s.ends_with(".mp3") {
        Ok((Some(AudioFormat::Mp3), Some(PathBuf::from(s))))
    } else if s.ends_with(".raw") || s.ends_with(".pcm") {
        Ok((Some(AudioFormat::Raw), Some(PathBuf::from(s))))
    } else {
        Err(format!(
            "invalid audio output '{}': must end in .aac, .wav, .mp3, .raw, or be --noaudio",
            s
        ))
    }
}

fn parse_offset_str(s: &str) -> Result<u64, String> {
    if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(rest, 16).map_err(|e| format!("invalid hex offset '{}': {}", s, e))
    } else if let Ok(val) = u64::from_str_radix(s, 16) {
        // Support hex without 0x prefix
        Ok(val)
    } else {
        s.parse::<u64>().map_err(|e| format!("invalid offset '{}': {}", s, e))
    }
}

fn run_analyze(cmd: AnalyzeCommand) -> std::io::Result<()> {
    println!("Dredge v0.1.0 — Media Analysis Phase");
    println!("Analyzing working reference: {}", cmd.working_input.display());

    let profile = DredgeProfile::analyze_file(&cmd.working_input)?;
    profile.save_to_file(&cmd.profile_out)?;

    if cmd.verbose {
        println!("  File size: {} bytes", profile.file_size);
    }

    println!();
    println!("✓ Successfully created profile: {}", cmd.profile_out.display());
    println!("  Brand: {}", profile.major_brand);
    if let Some(v) = &profile.video {
        println!("  Video: {} ({}x{}, timescale {})", v.codec, v.width, v.height, v.timescale);
        if let Some(avc) = &v.avc {
            println!("         SPS units: {}, PPS units: {}", avc.sps_list.len(), avc.pps_list.len());
        } else if let Some(hevc) = &v.hevc {
            println!("         VPS units: {}, SPS units: {}, PPS units: {}", hevc.vps_list.len(), hevc.sps_list.len(), hevc.pps_list.len());
        }
    }
    if let Some(a) = &profile.audio {
        println!("  Audio: {} ({} Hz, {} channels)", a.codec, a.sample_rate, a.channels);
    }

    println!();
    println!("Next Step (Recovery):");
    println!("  dredge <broken.mp4> recovered.h264 recovered.aac --profile {}", cmd.profile_out.display());
    println!();
    println!("Final Muxing (with FFmpeg):");
    println!("  ffmpeg -r 30 -i recovered.h264 -i recovered.aac -c copy final_video.mp4");

    Ok(())
}

fn run_recover(cmd: RecoverCommand) -> std::io::Result<()> {
    println!("Dredge v0.1.0 — Media Stream Recovery Phase");
    println!("Scanning damaged file: {}", cmd.broken_input.display());

    // Try loading profile if available
    let profile = if cmd.profile_path.exists() {
        match DredgeProfile::load_from_file(&cmd.profile_path) {
            Ok(p) => {
                println!("Loaded profile: {}", cmd.profile_path.display());
                Some(p)
            }
            Err(e) => {
                eprintln!("Warning: could not parse profile {}: {}", cmd.profile_path.display(), e);
                None
            }
        }
    } else {
        None
    };

    // 1. Recover Video
    if let (Some(fmt), Some(out_path)) = (cmd.video_format, &cmd.video_out) {
        println!("Extracting video stream to: {}", out_path.display());
        let stats = scanner::recover_video(
            &cmd.broken_input,
            out_path,
            fmt,
            profile.as_ref(),
            &cmd.options,
        )?;
        println!(
            "✓ Video stream recovered: {} bytes ({} NAL units / frames)",
            stats.video_bytes_written, stats.video_frames_recovered
        );
    }

    // 2. Recover Audio
    if let (Some(fmt), Some(out_path)) = (cmd.audio_format, &cmd.audio_out) {
        println!("Extracting audio stream to: {}", out_path.display());
        let stats = scanner::recover_audio(
            &cmd.broken_input,
            out_path,
            fmt,
            profile.as_ref(),
            &cmd.options,
        )?;
        println!(
            "✓ Audio stream recovered: {} bytes ({} frames / blocks)",
            stats.audio_bytes_written, stats.audio_frames_recovered
        );
    }

    println!();
    println!("══════════════════════════════════════════════════════════");
    println!("Recovery complete!");
    if let Some(v_path) = &cmd.video_out {
        println!("  Video output: {}", v_path.display());
    }
    if let Some(a_path) = &cmd.audio_out {
        println!("  Audio output: {}", a_path.display());
    }
    println!();
    println!("To mux the recovered streams into a playable video file:");
    if let (Some(v), Some(a)) = (&cmd.video_out, &cmd.audio_out) {
        println!("  ffmpeg -i {} -i {} -c copy recovered_video.mp4", v.display(), a.display());
    }
    println!("══════════════════════════════════════════════════════════");

    Ok(())
}

fn print_help() {
    println!("dredge 0.1.0");
    println!("Fast, memory-safe media recovery tool for unfinalized and damaged MP4/MOV video recordings.");
    println!();
    println!("USAGE:");
    println!("  dredge <working.mp4> --analyze [--out <profile.toml>]");
    println!("  dredge <broken.mp4> <out_video> <out_audio> [OPTIONS]");
    println!();
    println!("ANALYZE:");
    println!("  Analyzes a healthy reference recording made with the same camera/settings");
    println!("  and extracts codec headers (SPS/PPS for AVC/HEVC, AudioSpecificConfig for AAC)");
    println!("  into a structured recovery profile file.");
    println!();
    println!("RECOVER:");
    println!("  Scans a damaged or unfinalized video recording, extracts elementary video");
    println!("  and audio streams, attaches reconstructed codec headers, and writes the outputs.");
    println!();
    println!("ARGUMENTS:");
    println!("  <out_video>     Video stream destination (.h264, .hevc, or --novideo)");
    println!("  <out_audio>     Audio stream destination (.aac, .wav, .mp3, .raw, or --noaudio)");
    println!();
    println!("OPTIONS:");
    println!("  --profile <file>    Path to profile file (default: dredge-profile.toml)");
    println!("  --start <offset>    Start scan from byte offset (hex or decimal)");
    println!("  --end <offset>      End scan at byte offset (0 = EOF)");
    println!("  --verbose           Enable detailed diagnostic logging");
    println!("  --help, -h          Print this help message");
    println!("  --version, -V       Print version information");
    println!();
    println!("ATTRIBUTION:");
    println!("  Dredge is an independent open-source media recovery tool inspired by the");
    println!("  workflows of recover_mp4 (Dmitry Vasilyev) and untrunc (Federico Ponchio / anthwlock).");
}