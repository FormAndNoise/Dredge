#!/usr/bin/env python3
"""Dredge GUI wrapper (Tkinter)

A modern, fast graphical interface for Dredge:
1) Analyze a healthy reference file to create dredge-profile.toml
2) Recover elementary video (.h264/.hevc) and audio (.aac/.wav/.mp3) streams from a damaged recording
3) Optionally mux recovered streams into a playable MP4/MOV via FFmpeg

Designed for Windows. No external Python dependencies (pure Tkinter/standard library).
"""

from __future__ import annotations

import os
import sys
import shutil
import threading
import subprocess
from dataclasses import dataclass
from pathlib import Path
from queue import Queue, Empty

try:
    import tkinter as tk
    from tkinter import ttk, filedialog, messagebox
except Exception as e:
    raise SystemExit(f"Tkinter not available: {e}")


VIDEO_OUT = [
    ("H.264 / AVC elementary stream (.h264)", "h264"),
    ("H.265 / HEVC elementary stream (.hevc)", "hevc"),
    ("No video (--novideo)", "novideo"),
]

AUDIO_OUT = [
    ("AAC audio stream (.aac)", "aac"),
    ("WAV / PCM uncompressed audio (.wav)", "wav"),
    ("MP3 audio stream (.mp3)", "mp3"),
    ("Raw audio stream (.raw)", "raw"),
    ("No audio (--noaudio)", "noaudio"),
]


@dataclass
class CmdResult:
    ok: bool
    cmd: list[str]
    returncode: int
    stdout: str
    stderr: str


def is_windows() -> bool:
    return os.name == "nt"


def default_dredge_exe_dir() -> Path:
    if hasattr(sys, '_MEIPASS'):
        return Path(sys._MEIPASS)
    try:
        return Path(__file__).resolve().parent
    except Exception:
        return Path.cwd()


def pick_default_dredge_exe() -> Path | None:
    base = default_dredge_exe_dir()
    candidates = [
        base / "target" / "release" / "dredge.exe",
        base / "target" / "debug" / "dredge.exe",
        base / "dredge.exe",
        base / "rustore_mp4.exe",
        Path.cwd() / "target" / "release" / "dredge.exe",
        Path.cwd() / "dredge.exe",
    ]

    for c in candidates:
        if c.exists():
            return c

    return None


def find_ffmpeg() -> str | None:
    return shutil.which("ffmpeg")


def run_cmd(cmd: list[str], cwd: Path) -> CmdResult:
    try:
        p = subprocess.run(
            cmd,
            cwd=str(cwd),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            shell=False,
        )
        return CmdResult(
            ok=(p.returncode == 0),
            cmd=cmd,
            returncode=p.returncode,
            stdout=p.stdout or "",
            stderr=p.stderr or "",
        )
    except FileNotFoundError as e:
        return CmdResult(False, cmd, 127, "", str(e))
    except Exception as e:
        return CmdResult(False, cmd, 1, "", str(e))


class App(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("Dredge — Video Recovery Studio")
        self.geometry("980x740")
        self.minsize(900, 680)

        # Load window icon if present
        icon_path = default_dredge_exe_dir() / "Rustoremp4favicon.png"
        if icon_path.exists():
            try:
                icon_img = tk.PhotoImage(file=str(icon_path))
                self.iconphoto(True, icon_img)
            except Exception:
                pass

        self.queue: Queue = Queue()

        self.dredge_exe = tk.StringVar(value=str(pick_default_dredge_exe() or ""))
        self.ffmpeg_path = tk.StringVar(value=find_ffmpeg() or "")

        self.good_file = tk.StringVar(value="")
        self.bad_file = tk.StringVar(value="")
        self.out_dir = tk.StringVar(value=str(Path.cwd()))

        self.video_kind = tk.StringVar(value=VIDEO_OUT[0][1])
        self.audio_kind = tk.StringVar(value=AUDIO_OUT[0][1])

        self.fps = tk.StringVar(value="30")
        self.container_kind = tk.StringVar(value="mp4")

        self.start_offset = tk.StringVar(value="")
        self.end_offset = tk.StringVar(value="")

        self.base_name = tk.StringVar(value="recovered")

        self._build_ui()
        self._poll_queue()

    def _build_ui(self):
        pad = {"padx": 10, "pady": 6}

        # Top: tool paths
        frame_paths = ttk.LabelFrame(self, text="Core Tools")
        frame_paths.pack(fill="x", **pad)

        ttk.Label(frame_paths, text="Dredge Binary:").grid(row=0, column=0, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_paths, textvariable=self.dredge_exe).grid(row=0, column=1, sticky="ew", padx=8, pady=6)
        ttk.Button(frame_paths, text="Browse…", command=self._browse_dredge_exe).grid(row=0, column=2, padx=8, pady=6)

        ttk.Label(frame_paths, text="FFmpeg (optional for muxing):").grid(row=1, column=0, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_paths, textvariable=self.ffmpeg_path).grid(row=1, column=1, sticky="ew", padx=8, pady=6)
        ttk.Button(frame_paths, text="Browse…", command=self._browse_ffmpeg).grid(row=1, column=2, padx=8, pady=6)

        frame_paths.columnconfigure(1, weight=1)

        # Files
        frame_io = ttk.LabelFrame(self, text="File Selection")
        frame_io.pack(fill="x", **pad)

        ttk.Label(frame_io, text="1. Healthy Reference Video (same camera/settings):").grid(row=0, column=0, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_io, textvariable=self.good_file).grid(row=0, column=1, sticky="ew", padx=8, pady=6)
        ttk.Button(frame_io, text="Browse…", command=self._browse_good).grid(row=0, column=2, padx=8, pady=6)

        ttk.Label(frame_io, text="2. Damaged / Unfinalized Video to Recover:").grid(row=1, column=0, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_io, textvariable=self.bad_file).grid(row=1, column=1, sticky="ew", padx=8, pady=6)
        ttk.Button(frame_io, text="Browse…", command=self._browse_bad).grid(row=1, column=2, padx=8, pady=6)

        ttk.Label(frame_io, text="Destination Folder:").grid(row=2, column=0, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_io, textvariable=self.out_dir).grid(row=2, column=1, sticky="ew", padx=8, pady=6)
        ttk.Button(frame_io, text="Browse…", command=self._browse_outdir).grid(row=2, column=2, padx=8, pady=6)

        ttk.Label(frame_io, text="Output Base Name:").grid(row=3, column=0, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_io, textvariable=self.base_name, width=20).grid(row=3, column=1, sticky="w", padx=8, pady=6)

        frame_io.columnconfigure(1, weight=1)

        # Formats & Options
        frame_opts = ttk.LabelFrame(self, text="Stream & Format Options")
        frame_opts.pack(fill="x", **pad)

        ttk.Label(frame_opts, text="Video Stream Format:").grid(row=0, column=0, sticky="w", padx=8, pady=6)
        vid_menu = ttk.Combobox(frame_opts, values=[v[0] for v in VIDEO_OUT], state="readonly", width=36)
        vid_menu.grid(row=0, column=1, sticky="w", padx=8, pady=6)
        vid_menu.current(0)
        vid_menu.bind("<<ComboboxSelected>>", lambda e: self.video_kind.set(VIDEO_OUT[vid_menu.current()][1]))

        ttk.Label(frame_opts, text="Audio Stream Format:").grid(row=1, column=0, sticky="w", padx=8, pady=6)
        aud_menu = ttk.Combobox(frame_opts, values=[a[0] for a in AUDIO_OUT], state="readonly", width=36)
        aud_menu.grid(row=1, column=1, sticky="w", padx=8, pady=6)
        aud_menu.current(0)
        aud_menu.bind("<<ComboboxSelected>>", lambda e: self.audio_kind.set(AUDIO_OUT[aud_menu.current()][1]))

        ttk.Label(frame_opts, text="Start Offset (optional):").grid(row=0, column=2, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_opts, textvariable=self.start_offset, width=16).grid(row=0, column=3, sticky="w", padx=8, pady=6)

        ttk.Label(frame_opts, text="End Offset (optional, 0=EOF):").grid(row=1, column=2, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_opts, textvariable=self.end_offset, width=16).grid(row=1, column=3, sticky="w", padx=8, pady=6)

        # Actions Frame
        frame_actions = ttk.Frame(self)
        frame_actions.pack(fill="x", **pad)

        self.btn_analyze = ttk.Button(frame_actions, text="① Analyze Reference File", command=self._on_analyze)
        self.btn_analyze.pack(side="left", padx=6, pady=4)

        self.btn_recover = ttk.Button(frame_actions, text="② Recover Media Streams", command=self._on_recover)
        self.btn_recover.pack(side="left", padx=6, pady=4)

        self.btn_mux = ttk.Button(frame_actions, text="③ Mux into Playable MP4/MOV", command=self._on_mux)
        self.btn_mux.pack(side="left", padx=6, pady=4)

        self.btn_clear = ttk.Button(frame_actions, text="Clear Log", command=self._clear_log)
        self.btn_clear.pack(side="right", padx=6, pady=4)

        # Log Window
        frame_log = ttk.LabelFrame(self, text="Recovery Output & Diagnostics")
        frame_log.pack(fill="both", expand=True, **pad)

        self.txt_log = tk.Text(frame_log, wrap="word", height=14, font=("Consolas", 10), bg="#1e1e1e", fg="#e0e0e0")
        self.txt_log.pack(fill="both", expand=True, side="left", padx=6, pady=6)

        scroll = ttk.Scrollbar(frame_log, orient="vertical", command=self.txt_log.yview)
        scroll.pack(fill="y", side="right", pady=6)
        self.txt_log.config(yscrollcommand=scroll.set)

    def _browse_dredge_exe(self):
        f = filedialog.askopenfilename(title="Select Dredge Executable", filetypes=[("Executables", "*.exe"), ("All Files", "*.*")])
        if f:
            self.dredge_exe.set(f)

    def _browse_ffmpeg(self):
        f = filedialog.askopenfilename(title="Select FFmpeg Executable", filetypes=[("Executables", "*.exe"), ("All Files", "*.*")])
        if f:
            self.ffmpeg_path.set(f)

    def _browse_good(self):
        f = filedialog.askopenfilename(title="Select Healthy Reference Video", filetypes=[("Video Files", "*.mp4;*.mov;*.m4v;*.3gp"), ("All Files", "*.*")])
        if f:
            self.good_file.set(f)

    def _browse_bad(self):
        f = filedialog.askopenfilename(title="Select Damaged / Unfinalized Video", filetypes=[("Video Files", "*.mp4;*.mov;*.m4v;*.3gp;*.dat;*.raw"), ("All Files", "*.*")])
        if f:
            self.bad_file.set(f)

    def _browse_outdir(self):
        d = filedialog.askdirectory(title="Select Destination Directory")
        if d:
            self.out_dir.set(d)

    def _log(self, text: str):
        self.txt_log.insert("end", text)
        self.txt_log.see("end")

    def _clear_log(self):
        self.txt_log.delete("1.0", "end")

    def _poll_queue(self):
        try:
            while True:
                msg = self.queue.get_nowait()
                self._log(msg)
        except Empty:
            pass
        self.after(100, self._poll_queue)

    def _on_analyze(self):
        exe = Path(self.dredge_exe.get().strip())
        good = Path(self.good_file.get().strip())
        out_dir = Path(self.out_dir.get().strip())

        if not exe.exists():
            messagebox.showerror("Error", f"Dredge binary not found at:\n{exe}")
            return
        if not good.exists():
            messagebox.showerror("Error", "Please select a valid healthy reference video file.")
            return

        profile_path = out_dir / "dredge-profile.toml"

        def task():
            self.queue.put(f"\n[Dredge] Analyzing reference video: {good.name}...\n")
            cmd = [str(exe), str(good), "--analyze", "--out", str(profile_path)]
            res = run_cmd(cmd, out_dir)
            if res.ok:
                self.queue.put(res.stdout + "\n")
                self.queue.put(f"✓ Profile generated at: {profile_path}\n")
            else:
                self.queue.put(f"Error during analysis (code {res.returncode}):\n{res.stderr}\n{res.stdout}\n")

        threading.Thread(target=task, daemon=True).start()

    def _on_recover(self):
        exe = Path(self.dredge_exe.get().strip())
        bad = Path(self.bad_file.get().strip())
        out_dir = Path(self.out_dir.get().strip())
        base = self.base_name.get().strip() or "recovered"

        if not exe.exists():
            messagebox.showerror("Error", f"Dredge binary not found at:\n{exe}")
            return
        if not bad.exists():
            messagebox.showerror("Error", "Please select a damaged video file to recover.")
            return

        v_kind = self.video_kind.get()
        a_kind = self.audio_kind.get()

        v_arg = "--novideo" if v_kind == "novideo" else str(out_dir / f"{base}.{v_kind}")
        a_arg = "--noaudio" if a_kind == "noaudio" else str(out_dir / f"{base}.{a_kind}")
        profile_path = out_dir / "dredge-profile.toml"

        cmd = [str(exe), str(bad), v_arg, a_arg]
        if profile_path.exists():
            cmd.extend(["--profile", str(profile_path)])

        start = self.start_offset.get().strip()
        if start:
            cmd.extend(["--start", start])
        end = self.end_offset.get().strip()
        if end:
            cmd.extend(["--end", end])

        def task():
            self.queue.put(f"\n[Dredge] Starting media recovery on: {bad.name}...\n")
            res = run_cmd(cmd, out_dir)
            if res.ok:
                self.queue.put(res.stdout + "\n")
            else:
                self.queue.put(f"Error during recovery (code {res.returncode}):\n{res.stderr}\n{res.stdout}\n")

        threading.Thread(target=task, daemon=True).start()

    def _on_mux(self):
        ffmpeg = self.ffmpeg_path.get().strip() or find_ffmpeg()
        if not ffmpeg or not Path(ffmpeg).exists():
            messagebox.showerror("Error", "FFmpeg executable not found. Please specify the path to ffmpeg.exe.")
            return

        out_dir = Path(self.out_dir.get().strip())
        base = self.base_name.get().strip() or "recovered"
        v_kind = self.video_kind.get()
        a_kind = self.audio_kind.get()

        v_file = out_dir / f"{base}.{v_kind}"
        a_file = out_dir / f"{base}.{a_kind}"
        out_mp4 = out_dir / f"{base}_final.mp4"

        cmd = [str(ffmpeg), "-y"]
        if v_kind != "novideo" and v_file.exists():
            cmd.extend(["-r", self.fps.get().strip() or "30", "-i", str(v_file)])
        if a_kind != "noaudio" and a_file.exists():
            cmd.extend(["-i", str(a_file)])

        cmd.extend(["-c", "copy", str(out_mp4)])

        def task():
            self.queue.put(f"\n[FFmpeg] Muxing recovered streams into: {out_mp4.name}...\n")
            res = run_cmd(cmd, out_dir)
            if res.ok:
                self.queue.put(f"✓ Playable video successfully created: {out_mp4}\n")
            else:
                self.queue.put(f"FFmpeg muxing error:\n{res.stderr}\n")

        threading.Thread(target=task, daemon=True).start()


if __name__ == "__main__":
    app = App()
    app.mainloop()
