#!/usr/bin/env python3
"""Rustore MP4 GUI wrapper (Tkinter)

This wraps Dmitry Vasilyev's rustore_mp4.exe (CLI) with a simple GUI:
1) Analyze a known-good file to create video.hdr/audio.hdr
2) Recover streams from a corrupted file
3) Optionally mux recovered streams into MP4/MOV via ffmpeg

Designed for Windows. No external Python deps.
"""

from __future__ import annotations

import os
import sys
import shlex
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


TEMPLATES = [
    ("Auto (let tool decide)", ""),
    ("GoPro / Ambarella (MP4 default)", "--gopro4"),
    ("Ambarella (no GoPro extras)", "--ambarella"),
    ("Lav/ffmpeg", "--lav"),
    ("QuickTime (MOV default)", "--qt"),
    ("Generic / external / OBS / other", "--ext"),
    ("Sony", "--sony"),
    ("Panasonic", "--panasonic"),
    ("Canon EOS Mk II/7D/550D", "--eos"),
    ("Canon EOS Mk III/7D Mk II/100D", "--eos3"),
    ("Canon EOS Mk IV", "--eos4"),
    ("Canon (SX / EOS 80D)", "--canon"),
    ("iPhone 4", "--iphone4"),
    ("Blackmagic", "--blackmagic"),
    ("DJI Phantom H.264", "--dji.avc"),
    ("DJI Phantom H.265", "--dji.hvc"),
    ("Generic HEVC/H.265 templates", "--hevc"),
    ("Novatek DVR", "--novatek"),
]

VIDEO_OUT = [
    ("H.264 elementary stream (.h264)", "h264"),
    ("H.265/HEVC elementary stream (.hevc)", "hevc"),
    ("No video", "novideo"),
]

AUDIO_OUT = [
    ("AAC (.aac)", "aac"),
    ("WAV/PCM (.wav)", "wav"),
    ("MP3 (.mp3)", "mp3"),
    ("RAW (.raw)", "raw"),
    ("No audio", "noaudio"),
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


def default_recover_exe_dir() -> Path:
    # Handle PyInstaller _MEIPASS bundle directory
    if hasattr(sys, '_MEIPASS'):
        return Path(sys._MEIPASS)
    # Prefer the folder where this script lives.
    try:
        return Path(__file__).resolve().parent
    except Exception:
        return Path.cwd()


def pick_default_recover_exe() -> Path | None:
    base = default_recover_exe_dir()
    # On most systems rustore_mp4.exe is the x64 build and rustore_mp4_x86.exe is 32-bit.
    candidates = [base / "rustore_mp4.exe", base / "rustore_mp4_x86.exe"]

    for c in candidates:
        if c.exists():
            return c

    # Fallback: search next to current working dir.
    candidates = [Path.cwd() / "rustore_mp4.exe", Path.cwd() / "rustore_mp4_x86.exe"]
    for c in candidates:
        if c.exists():
            return c

    return None


def find_ffmpeg() -> str | None:
    # Return full path if found in PATH.
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
        self.title("Recover MP4 - Simple GUI")
        self.geometry("980x720")
        self.minsize(900, 650)
        
        # Load and set window icon
        icon_path = default_recover_exe_dir() / "Rustoremp4favicon.png"
        if icon_path.exists():
            try:
                icon_img = tk.PhotoImage(file=str(icon_path))
                self.iconphoto(True, icon_img)
            except Exception:
                pass


        self.queue: Queue = Queue()

        self.recover_exe = tk.StringVar(value=str(pick_default_recover_exe() or ""))
        self.ffmpeg_path = tk.StringVar(value=find_ffmpeg() or "")

        self.good_file = tk.StringVar(value="")
        self.bad_file = tk.StringVar(value="")
        self.out_dir = tk.StringVar(value=str(Path.cwd()))

        self.template_label = tk.StringVar(value=TEMPLATES[0][0])
        self.video_kind = tk.StringVar(value=VIDEO_OUT[0][1])
        self.audio_kind = tk.StringVar(value=AUDIO_OUT[0][1])

        self.fps = tk.StringVar(value="30")
        self.container_kind = tk.StringVar(value="mp4")  # mp4 or mov

        self.extra_options = tk.StringVar(value="")
        self.start_hex = tk.StringVar(value="")
        self.end_hex = tk.StringVar(value="")

        self.base_name = tk.StringVar(value="recovered")

        self._build_ui()
        self._poll_queue()

        if not is_windows():
            self._log("Heads up: this GUI is designed for Windows (rustore_mp4.exe).\n"
                      "On Linux/macOS you would need Wine, or a native build.\n\n")

    def _build_ui(self):
        pad = {"padx": 10, "pady": 6}

        # Top: tool paths
        frame_paths = ttk.LabelFrame(self, text="Tools")
        frame_paths.pack(fill="x", **pad)

        ttk.Label(frame_paths, text="Rustore MP4 executable:").grid(row=0, column=0, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_paths, textvariable=self.recover_exe).grid(row=0, column=1, sticky="ew", padx=8, pady=6)
        ttk.Button(frame_paths, text="Browse…", command=self._browse_recover_exe).grid(row=0, column=2, padx=8, pady=6)

        ttk.Label(frame_paths, text="ffmpeg (optional, for muxing):").grid(row=1, column=0, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_paths, textvariable=self.ffmpeg_path).grid(row=1, column=1, sticky="ew", padx=8, pady=6)
        ttk.Button(frame_paths, text="Browse…", command=self._browse_ffmpeg).grid(row=1, column=2, padx=8, pady=6)

        frame_paths.columnconfigure(1, weight=1)

        # Inputs/Outputs
        frame_io = ttk.LabelFrame(self, text="Files")
        frame_io.pack(fill="x", **pad)

        ttk.Label(frame_io, text="Good reference MP4/MOV (same camera/settings):").grid(row=0, column=0, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_io, textvariable=self.good_file).grid(row=0, column=1, sticky="ew", padx=8, pady=6)
        ttk.Button(frame_io, text="Browse…", command=self._browse_good).grid(row=0, column=2, padx=8, pady=6)

        ttk.Label(frame_io, text="Corrupted MP4/MOV to recover:").grid(row=1, column=0, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_io, textvariable=self.bad_file).grid(row=1, column=1, sticky="ew", padx=8, pady=6)
        ttk.Button(frame_io, text="Browse…", command=self._browse_bad).grid(row=1, column=2, padx=8, pady=6)

        ttk.Label(frame_io, text="Output folder (headers + recovered streams go here):").grid(row=2, column=0, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_io, textvariable=self.out_dir).grid(row=2, column=1, sticky="ew", padx=8, pady=6)
        ttk.Button(frame_io, text="Browse…", command=self._browse_outdir).grid(row=2, column=2, padx=8, pady=6)

        ttk.Label(frame_io, text="Output base name:").grid(row=3, column=0, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_io, textvariable=self.base_name, width=20).grid(row=3, column=1, sticky="w", padx=8, pady=6)

        frame_io.columnconfigure(1, weight=1)

        # Options
        frame_opts = ttk.LabelFrame(self, text="Options")
        frame_opts.pack(fill="x", **pad)

        ttk.Label(frame_opts, text="Template preset:").grid(row=0, column=0, sticky="w", padx=8, pady=6)
        tpl_menu = ttk.Combobox(frame_opts, values=[t[0] for t in TEMPLATES], textvariable=self.template_label, state="readonly")
        tpl_menu.grid(row=0, column=1, sticky="w", padx=8, pady=6)

        ttk.Label(frame_opts, text="Video output:").grid(row=1, column=0, sticky="w", padx=8, pady=6)
        vid_menu = ttk.Combobox(frame_opts, values=[v[0] for v in VIDEO_OUT], state="readonly")
        vid_menu.grid(row=1, column=1, sticky="w", padx=8, pady=6)
        vid_menu.current(0)
        vid_menu.bind("<<ComboboxSelected>>", lambda e: self.video_kind.set(VIDEO_OUT[vid_menu.current()][1]))

        ttk.Label(frame_opts, text="Audio output:").grid(row=2, column=0, sticky="w", padx=8, pady=6)
        aud_menu = ttk.Combobox(frame_opts, values=[a[0] for a in AUDIO_OUT], state="readonly")
        aud_menu.grid(row=2, column=1, sticky="w", padx=8, pady=6)
        aud_menu.current(0)
        aud_menu.bind("<<ComboboxSelected>>", lambda e: self.audio_kind.set(AUDIO_OUT[aud_menu.current()][1]))

        ttk.Label(frame_opts, text="Read start hex (optional):").grid(row=0, column=2, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_opts, textvariable=self.start_hex, width=16).grid(row=0, column=3, sticky="w", padx=8, pady=6)

        ttk.Label(frame_opts, text="Read end hex (optional, 0=EOF):").grid(row=1, column=2, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_opts, textvariable=self.end_hex, width=16).grid(row=1, column=3, sticky="w", padx=8, pady=6)

        ttk.Label(frame_opts, text="Extra Rustore MP4 options (paste flags here):").grid(row=2, column=2, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_opts, textvariable=self.extra_options, width=40).grid(row=2, column=3, sticky="ew", padx=8, pady=6)

        frame_opts.columnconfigure(3, weight=1)

        # Mux options
        frame_mux = ttk.LabelFrame(self, text="Mux to MP4/MOV (optional)")
        frame_mux.pack(fill="x", **pad)

        ttk.Label(frame_mux, text="FPS (needed for ffmpeg -r):").grid(row=0, column=0, sticky="w", padx=8, pady=6)
        ttk.Entry(frame_mux, textvariable=self.fps, width=10).grid(row=0, column=1, sticky="w", padx=8, pady=6)

        ttk.Label(frame_mux, text="Container:").grid(row=0, column=2, sticky="w", padx=8, pady=6)
        cont = ttk.Combobox(frame_mux, values=["mp4", "mov"], textvariable=self.container_kind, state="readonly", width=8)
        cont.grid(row=0, column=3, sticky="w", padx=8, pady=6)

        ttk.Label(frame_mux, text="Note: MP4 won't accept PCM/WAV audio. Use MOV for WAV.").grid(row=0, column=4, sticky="w", padx=8, pady=6)
        frame_mux.columnconfigure(4, weight=1)

        # Actions
        frame_actions = ttk.Frame(self)
        frame_actions.pack(fill="x", **pad)

        ttk.Button(frame_actions, text="1) Analyze good file", command=self._analyze).pack(side="left", padx=6)
        ttk.Button(frame_actions, text="2) Recover streams", command=self._recover).pack(side="left", padx=6)
        ttk.Button(frame_actions, text="3) Mux to MP4/MOV (ffmpeg)", command=self._mux).pack(side="left", padx=6)

        ttk.Button(frame_actions, text="Open output folder", command=self._open_outdir).pack(side="right", padx=6)

        # Log
        frame_log = ttk.LabelFrame(self, text="Log")
        frame_log.pack(fill="both", expand=True, **pad)

        self.log = tk.Text(frame_log, wrap="word", height=18)
        self.log.pack(fill="both", expand=True, padx=8, pady=8)

        # Simple tags
        self.log.tag_configure("ok", foreground="#0a6")
        self.log.tag_configure("err", foreground="#c00")
        self.log.tag_configure("cmd", foreground="#06c")

        self._log("Tip: If Analyze prints a specific flag to use (like --gopro4/--qt/etc), set it in Template preset or paste into Extra options.\n\n")

    def _log(self, text: str, tag: str | None = None):
        self.log.insert("end", text, tag if tag else ())
        self.log.see("end")

    def _poll_queue(self):
        try:
            while True:
                item = self.queue.get_nowait()
                kind = item.get("kind")
                if kind == "log":
                    self._log(item["text"], item.get("tag"))
                elif kind == "result":
                    self._handle_result(item["result"])
        except Empty:
            pass
        self.after(120, self._poll_queue)

    def _handle_result(self, res: CmdResult):
        self._log("\n$ " + " ".join(shlex.quote(c) for c in res.cmd) + "\n", "cmd")
        if res.stdout:
            self._log(res.stdout + ("\n" if not res.stdout.endswith("\n") else ""))
        if res.stderr:
            self._log(res.stderr + ("\n" if not res.stderr.endswith("\n") else ""), "err")
        if res.ok:
            self._log(f"[OK] exit code {res.returncode}\n", "ok")
        else:
            self._log(f"[ERROR] exit code {res.returncode}\n", "err")

    def _browse_recover_exe(self):
        p = filedialog.askopenfilename(title="Select Rustore MP4 executable", filetypes=[("Rustore MP4", "*.exe"), ("All files", "*")])
        if p:
            self.recover_exe.set(p)

    def _browse_ffmpeg(self):
        p = filedialog.askopenfilename(title="Select ffmpeg executable", filetypes=[("ffmpeg", "ffmpeg.exe"), ("Executable", "*.exe"), ("All files", "*")])
        if p:
            self.ffmpeg_path.set(p)

    def _browse_good(self):
        p = filedialog.askopenfilename(title="Select a good reference file", filetypes=[("Video", "*.mp4 *.mov *.3gp"), ("All files", "*")])
        if p:
            self.good_file.set(p)

    def _browse_bad(self):
        p = filedialog.askopenfilename(title="Select a corrupted file", filetypes=[("Video", "*.mp4 *.mov *.3gp"), ("All files", "*")])
        if p:
            self.bad_file.set(p)

    def _browse_outdir(self):
        p = filedialog.askdirectory(title="Select output folder")
        if p:
            self.out_dir.set(p)

    def _open_outdir(self):
        out = Path(self.out_dir.get()).expanduser()
        if not out.exists():
            messagebox.showerror("Output folder", "Output folder does not exist.")
            return
        try:
            if is_windows():
                os.startfile(str(out))  # type: ignore[attr-defined]
            else:
                subprocess.Popen(["xdg-open", str(out)])
        except Exception as e:
            messagebox.showerror("Open folder", str(e))

    def _template_flag(self) -> str:
        label = self.template_label.get().strip()
        for k, flag in TEMPLATES:
            if k == label:
                return flag
        return ""

    def _validate_exe(self) -> Path | None:
        exe = Path(self.recover_exe.get()).expanduser()
        if not exe.exists():
            messagebox.showerror("Rustore MP4", "Rustore MP4 executable not found. Browse to rustore_mp4.exe")
            return None
        return exe

    def _validate_outdir(self) -> Path | None:
        out = Path(self.out_dir.get()).expanduser()
        if not out.exists():
            try:
                out.mkdir(parents=True, exist_ok=True)
            except Exception as e:
                messagebox.showerror("Output folder", f"Can't create output folder: {e}")
                return None
        return out

    def _threaded_run(self, cmd: list[str], cwd: Path):
        def worker():
            self.queue.put({"kind": "log", "text": "\nRunning…\n"})
            res = run_cmd(cmd, cwd=cwd)
            self.queue.put({"kind": "result", "result": res})
        threading.Thread(target=worker, daemon=True).start()

    def _analyze(self):
        exe = self._validate_exe()
        out = self._validate_outdir()
        if not exe or not out:
            return

        good = Path(self.good_file.get()).expanduser()
        if not good.exists():
            messagebox.showerror("Analyze", "Pick a good reference MP4/MOV first.")
            return

        cmd = [str(exe), str(good)]
        flag = self._template_flag()
        if flag:
            cmd.append(flag)
        cmd.append("--analyze")

        self._threaded_run(cmd, cwd=out)

    def _recover(self):
        exe = self._validate_exe()
        out = self._validate_outdir()
        if not exe or not out:
            return

        bad = Path(self.bad_file.get()).expanduser()
        if not bad.exists():
            messagebox.showerror("Recover", "Pick the corrupted MP4/MOV first.")
            return

        # Warn if headers missing.
        v_hdr = out / "video.hdr"
        a_hdr = out / "audio.hdr"
        if not v_hdr.exists() and not a_hdr.exists():
            if not messagebox.askyesno(
                "Headers not found",
                "video.hdr/audio.hdr not found in output folder.\n\n"
                "Normally you should run 'Analyze good file' first.\n"
                "Continue anyway?",
            ):
                return

        base = self.base_name.get().strip() or "recovered"

        vkind = self.video_kind.get()
        akind = self.audio_kind.get()

        # Build output names.
        if vkind == "h264":
            vout = out / f"{base}.h264"
        elif vkind == "hevc":
            vout = out / f"{base}.hevc"
        else:
            vout = None

        if akind == "aac":
            aout = out / f"{base}.aac"
        elif akind == "wav":
            aout = out / f"{base}.wav"
        elif akind == "mp3":
            aout = out / f"{base}.mp3"
        elif akind == "raw":
            aout = out / f"{base}.raw"
        else:
            aout = None

        cmd = [str(exe), str(bad)]

        # Video arg
        if vout is None:
            cmd.append("--novideo")
        else:
            cmd.append(str(vout))

        # Audio arg
        if aout is None:
            cmd.append("--noaudio")
        else:
            cmd.append(str(aout))

        # Template
        flag = self._template_flag()
        if flag:
            cmd.append(flag)

        # Start/end hex
        if self.start_hex.get().strip():
            cmd += ["--start", self.start_hex.get().strip()]
        if self.end_hex.get().strip():
            cmd += ["--end", self.end_hex.get().strip()]

        # Extra options
        extra = self.extra_options.get().strip()
        if extra:
            # Split like a shell command line (handles quotes).
            try:
                cmd += shlex.split(extra)
            except ValueError:
                messagebox.showerror("Extra options", "Couldn't parse Extra options. Check quotes/spaces.")
                return

        self._threaded_run(cmd, cwd=out)

    def _mux(self):
        out = self._validate_outdir()
        if not out:
            return

        ff = self.ffmpeg_path.get().strip() or find_ffmpeg() or ""
        if not ff:
            messagebox.showerror("ffmpeg", "ffmpeg not found. Install ffmpeg or browse to ffmpeg.exe")
            return

        base = self.base_name.get().strip() or "recovered"

        # Locate recovered streams based on selected kinds.
        vkind = self.video_kind.get()
        akind = self.audio_kind.get()

        if vkind == "h264":
            vfile = out / f"{base}.h264"
        elif vkind == "hevc":
            vfile = out / f"{base}.hevc"
        else:
            messagebox.showerror("Mux", "Muxing requires a video stream output (h264/hevc).")
            return

        if akind == "aac":
            afile = out / f"{base}.aac"
        elif akind == "wav":
            afile = out / f"{base}.wav"
        elif akind == "mp3":
            afile = out / f"{base}.mp3"
        elif akind == "raw":
            messagebox.showerror("Mux", "RAW audio isn't self-describing; muxing automatically isn't supported here. Use WAV/AAC/MP3 instead.")
            return
        else:
            messagebox.showerror("Mux", "Muxing requires an audio stream output (aac/wav/mp3).")
            return

        if not vfile.exists():
            messagebox.showerror("Mux", f"Video stream not found: {vfile}")
            return
        if not afile.exists():
            messagebox.showerror("Mux", f"Audio stream not found: {afile}")
            return

        fps = self.fps.get().strip() or "30"
        container = self.container_kind.get().strip().lower() or "mp4"

        # MP4 doesn't support PCM/WAV.
        if akind == "wav" and container == "mp4":
            if messagebox.askyesno("Container", "WAV/PCM audio can't go in MP4. Switch container to MOV?"):
                container = "mov"
                self.container_kind.set("mov")
            else:
                return

        outfile = out / f"{base}.{container}"

        cmd = [ff, "-y", "-r", fps, "-i", str(vfile), "-i", str(afile)]

        # Copy streams; AAC needs bitstream filter if it was ADTS.
        if akind == "aac":
            cmd += ["-bsf:a", "aac_adtstoasc"]

        cmd += ["-c:v", "copy", "-c:a", "copy", str(outfile)]

        self._threaded_run(cmd, cwd=out)


def main():
    app = App()
    app.mainloop()


if __name__ == "__main__":
    main()
