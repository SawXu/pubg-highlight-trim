# ffmpeg vendor directory

GitHub Actions and `scripts/download_ffmpeg.ps1` populate this directory with Windows FFmpeg runtime files before packaging.

The pinned download is BtbN's immutable monthly release asset recorded in `scripts/ffmpeg-lock.json`, so bundled FFmpeg/FFprobe are GPL-3.0-or-later according to `ffmpeg.exe -L`. BtbN retains monthly builds for two years; refresh this pin before its September 2028 retention deadline.

Expected runtime files include:

- `ffmpeg.exe`
- `ffprobe.exe`
- FFmpeg DLLs used by the shared Windows build

The binary payload is intentionally not committed to git. See `THIRD_PARTY_NOTICES.md` at the repository root for source links and license notes.
