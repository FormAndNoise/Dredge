# Dredge

<p align="center">
  <img src="Rustoremp4favicon.png" alt="Dredge Logo" width="160"/>
</p>

**Dredge** is a fast, memory-safe media recovery tool for extracting valid H.264/AVC, H.265/HEVC video streams, and AAC/WAV/MP3 audio streams from corrupted, truncated, or unfinalized MP4 and MOV recordings.

---

## ⚡ Key Highlights

- **Recursive ISO BMFF Box Parser**: Recursively traverses nested atom structures (`moov` → `trak` → `mdia` → `minf` → `stbl` → `stsd` → `avc1`/`hvc1`) to accurately extract codec configuration records (`avcC`, `hvcC`, `esds`).
- **Annex B Parameter Reconstruction**: Extracts SPS/PPS (for AVC) and VPS/SPS/PPS (for HEVC) from healthy reference files and injects Annex B start codes (`00 00 00 01`) so recovered video elementary streams decode properly in standard players.
- **Synthesized ADTS Headers**: Converts raw AAC access units into standard ADTS audio frames using sample rate and channel configurations extracted from reference recordings.
- **Memory-Safe Streaming Scanner**: Uses an 8MB buffered sliding window with overlap to scan massive 4K/8K multi-gigabyte recordings without Out-Of-Memory (OOM) crashes.
- **Standalone GUI & CLI**: Includes both a high-performance native CLI (`dredge`) and a standalone graphical interface (`dredge_gui.py`).

---

## 🖥️ Graphical Interface (GUI)

Run `python dredge_gui.py` (or launch the compiled `dredge_gui.exe`):

1. **Step 1 — Analyze**: Select a working, healthy reference video recorded with the same camera and resolution settings. Click **Analyze Reference File** to generate `dredge-profile.toml`.
2. **Step 2 — Recover**: Select your damaged/unfinalized video file and output destination. Click **Recover Media Streams** to extract elementary video (`.h264` / `.hevc`) and audio (`.aac` / `.wav` / `.mp3`) files.
3. **Step 3 — Mux (Optional)**: Click **Mux into Playable MP4/MOV** to merge the recovered streams with FFmpeg.

---

## ⌨️ Command-Line Interface (CLI)

### 1. Analyze Working Reference Video
```bash
dredge working.mp4 --analyze --out dredge-profile.toml
```

### 2. Recover Damaged Recording
```bash
dredge broken.mp4 recovered.h264 recovered.aac --profile dredge-profile.toml
```

### 3. Mux Streams (with FFmpeg)
```bash
ffmpeg -r 30 -i recovered.h264 -i recovered.aac -c copy final_recovered.mp4
```

### Additional Options:
- `--profile <file>`: Custom path to camera profile TOML file.
- `--start <offset>`: Start byte offset to bypass corrupted file headers.
- `--end <offset>`: End byte offset (0 = EOF).
- `--verbose`: Enable detailed logging.

---

## 🛠️ Building from Source

### Prerequisites
- **Rust & Cargo** (stable 1.70+)
- **Python 3.10+** (for GUI)

### Build the Rust Binary
```powershell
cargo build --release
```
The compiled binary will be placed at `target/release/dredge.exe`.

### Run Automated Tests
```powershell
cargo test
```

---

## 📜 Attribution & Lineage

Dredge is an independent open-source media recovery tool inspired by the analysis workflows of `recover_mp4` (by Dmitry Vasilyev) and `untrunc` (by Federico Ponchio and anthwlock). Dredge is not affiliated with, endorsed by, or derived from the proprietary source code of those projects.
