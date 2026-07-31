# Rustore MP4

<p align="center">
  <img src="Rustoremp4favicon.png" alt="Rustore MP4 Logo" width="200"/>
</p>

**Rustore MP4** is a modern, unified application for recovering H.264/AVC and H.265/HEVC video streams and audio streams from corrupted or unfinalized MP4, MOV, and 3GP files. 

This repository contains a full **cleanroom Rust rewrite** of the legendary `recover_mp4` utility (originally by Dmitry Vasilyev), fully integrated with a **Python-based Tkinter GUI wrapper** for ease of use. The entire stack can be compiled and bundled into a single standalone executable.

---

## 🚀 Unified Build System

This repository is uniquely flattened and configured to be compiled into a single, unified `.exe` binary that contains both the Rust engine and the Python graphical interface.

### Compiling the Unified Binary (Windows)

Ensure you have **Rust/Cargo** and **Python 3.10+** installed on your system.

1. Open PowerShell and navigate to the project directory.
2. Run the build script:
   ```powershell
   .\build.ps1
   ```
3. The script will automatically:
   - Compile the Rust engine using `cargo build --release`
   - Use `PyInstaller` to seamlessly package the Python GUI, the `.ico` graphic, and the Rust engine into a single executable.
4. Your finished standalone program will be output to the `dist/` directory!

---

## 🖥️ Usage

### Using the Graphical User Interface (GUI)
Simply double-click the compiled `rustore_mp4_gui.exe` (found in the `dist` folder after building). 
1. **Analyze:** Select a "good", fully functional reference video from the same camera/settings. The program will generate `video.hdr` and `audio.hdr` structural files.
2. **Recover:** Select your corrupted video file and an output directory. Click "Recover Streams" to extract the raw `.h264`/`.hevc` and audio streams.
3. **Mux (Optional):** Use the interface to stitch the raw audio and video back into a playable MP4 or MOV file (requires `ffmpeg`).

### Command-Line Interface (CLI)

The underlying engine functions as a 1:1 drop-in replacement for the original tool. If you prefer the terminal, you can run the compiled `rustore_mp4.exe` manually.

**Phase 1: Analyze**
```bash
rustore_mp4 in_good_similar.mp4 --analyze
```
*(Generates `video.hdr` and `audio.hdr` in the current working directory).*

**Phase 2: Recover**
```bash
rustore_mp4 in_corrupted.mp4 out_video.h264 out_audio.aac [options]
```

**Common Flags:**
- `--gopro4`: Use Ambarella templates and detect GoPro4 specific data.
- `--qt`: Use QuickTime templates (default for MOV files).
- `--hevc`: Use generic HEVC/H.265 templates.
- `--ext`: Generic templates for OBS, smartphones, and miscellaneous cameras.

---

## 📜 Credits & Background
Built upon the reverse-engineering principles of the original closed-source `recover_mp4` utility (v1.92) by Dmitry Vasilyev. 

**Special Acknowledgments:**
A massive thank you to the open-source heroes who did the hard reverse-engineering work that makes our camera-specific parsing rules possible:
- **Federico Ponchio** for creating the incredible [`untrunc`](https://github.com/ponchio/untrunc) tool, which provided the mathematical blueprint for parsing GoPro (`gpmd`, `fdsc`), Timecode (`tmcd`), and Camera Metadata (`camm`) chunks.
- **anthwlock** for maintaining and expanding the highly successful [`untrunc` fork](https://github.com/anthwlock/untrunc) which served as the primary reference for maximizing camera compatibility.

The Rust implementation provides modern memory safety, incredible parsing speed, and true cross-platform compatibility, ensuring these critical archival and recovery techniques are preserved in an open, unified ecosystem.
