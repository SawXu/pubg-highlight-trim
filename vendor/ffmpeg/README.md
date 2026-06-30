# ffmpeg vendor directory

GitHub Actions and `scripts/download_ffmpeg.ps1` populate this directory with Windows FFmpeg runtime files before packaging.

The default download is BtbN's `ffmpeg-master-latest-win64-gpl-shared.zip`, so bundled FFmpeg/FFprobe are GPL-3.0-or-later according to `ffmpeg.exe -L`.

Expected runtime files include:

- `ffmpeg.exe`
- `ffprobe.exe`
- FFmpeg DLLs used by the shared Windows build

The binary payload is intentionally not committed to git. See `THIRD_PARTY_NOTICES.md` at the repository root for source links and license notes.
