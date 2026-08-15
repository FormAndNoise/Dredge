Dredge GUI (Simple & Fast Video Recovery)

What this is
- A clean, lightweight Tkinter GUI wrapper for the native Dredge media recovery engine (dredge.exe).
- Makes the 3-step recovery workflow simple:
  1) Analyze a healthy reference video -> creates dredge-profile.toml with extracted SPS/PPS/AAC parameters
  2) Recover elementary streams from the damaged recording -> outputs .h264/.hevc and .aac/.wav/.mp3
  3) (Optional) Mux the streams back into a playable MP4/MOV using FFmpeg

Requirements (Windows)
- Python 3.10+ (standard library only, no pip packages required)
- dredge.exe (compiled in target/release/ or placed in the same folder)
- Optional for muxing: ffmpeg.exe in PATH or selected in the GUI

How to use
1) Run:
   python dredge_gui.py
2) In the GUI:
   - Choose Destination Folder (where profile and recovered streams will be written)
   - Select a healthy reference video and click "① Analyze Reference File"
   - Select the corrupted video and click "② Recover Media Streams"
   - If you want a finished MP4/MOV file, click "③ Mux into Playable MP4/MOV"
