# AGENTS.md — Dredge

Fast, memory-safe media recovery tool for unfinalized and damaged MP4/MOV recordings.

## Brand Identity & Standards
- **Product Name**: **Dredge** (Repo: `dredge`, Binary: `dredge` / `moov`)
- **Accent**: Signal Amber (`#C4922A` / Dark: `#D9A93F`) strictly on `#0B0B0B` Void ground.
- **Job Line**: *"Extracts raw elementary video and audio streams from unfinalized MP4/MOV recordings."*
- **Symbol**: 24×24u, 1.75u stroke, maritime salvage grapnel hooking 1 solid recovered stream pip.
- **Vocabulary**: Working file (`--working`), Broken file (`--broken`), Rebuild.
- **Suite Context**: Form & Noise Atelier Loose Endorsed Family. Space Grotesk (Wordmarks), Inter (GUI), IBM Plex Mono (CLI).

## Technical Architecture
- **Recursive Box Parser**: Traverses ISO BMFF hierarchy (`moov` → `trak` → `mdia` → `minf` → `stbl` → `stsd` → `avc1`/`hvc1`).
- **Annex B SPS/PPS & ADTS Synthesis**: Reconstructs sequence parameter sets and synthesizes 7-byte ADTS headers.
- **Memory Safety**: 8MB sliding chunk reader with overlap buffer to prevent OOM on large 4K/8K video dumps.
- **Profile Format**: `dredge-profile.toml`.

## Build & Test Commands
```powershell
cargo test            # Run unit and integration tests (6 tests)
cargo build --release # Build release executable (target/release/dredge.exe)
python dredge_gui.py  # Launch graphical interface
```
