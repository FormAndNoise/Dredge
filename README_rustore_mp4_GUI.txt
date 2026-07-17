recover_mp4 GUI (simple wrapper)

What this is
- A small Tkinter GUI that wraps recover_mp4.exe (the command-line tool in your ZIP).
- It makes the normal workflow clickable:
  1) Analyze a known-good file -> creates video.hdr + audio.hdr
  2) Recover streams from the corrupted file -> outputs .h264/.hevc and .aac/.wav/.mp3
  3) (Optional) Mux the streams back into MP4/MOV using ffmpeg

Requirements (Windows)
- Python 3.10+ recommended (from python.org). During install, check: "Add Python to PATH".
- recover_mp4.exe in the same folder as this GUI (or pick it inside the GUI).
- Optional for the "Mux" step: ffmpeg.exe (either in PATH or pick it inside the GUI).

How to use
1) Put these files in ONE folder:
   - recover_mp4.exe (or recover_mp4_x86.exe)
   - recover_mp4_gui.py
2) Run:
   - Double-click recover_mp4_gui.py, OR
   - Open Command Prompt in that folder and run:
       python recover_mp4_gui.py
3) In the GUI:
   - Choose an Output folder (important: that’s where video.hdr/audio.hdr will be saved)
   - Pick a good reference MP4/MOV and click Analyze
   - Pick the corrupted file and click Recover
   - If you want a playable file, set FPS + container (MP4/MOV) and click Mux (ffmpeg)

Notes / gotchas
- The "good reference" file should be from the same device/settings as the broken one (same resolution, fps, codec).
- The tool writes video.hdr/audio.hdr into the Output folder. Don’t delete them between Analyze and Recover.
- WAV/PCM audio cannot go into MP4. If you recover WAV, mux into MOV.

Optional: make a standalone EXE (so non-technical users can just run it)
- Install pyinstaller:
    python -m pip install pyinstaller
- Build:
    pyinstaller --onefile --noconsole recover_mp4_gui.py
- Your EXE will be in: dist\recover_mp4_gui.exe

Troubleshooting
- If Recover finds no audio/video, try changing the Template dropdown (Sony/Panasonic/Canon/Generic/qt/ext).
- If your camera is odd, click Analyze and copy any suggested flags from the output into "Extra options".
- If ffmpeg isn’t found, download it and set the path in the GUI.
