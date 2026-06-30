# ffmpeg vendor directory

GitHub Actions and `scripts/download_ffmpeg.ps1` populate this directory with Windows ffmpeg runtime files before packaging.

Expected files include:

- `ffmpeg.exe`
- `ffprobe.exe`
- FFmpeg DLLs when using shared Windows builds

The binary payload is intentionally not committed to git.
