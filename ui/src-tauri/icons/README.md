# Application icon

Selected artwork: a warm-gold clapperboard, centered play symbol, and highlight sparkle on a dark rounded background. The composition represents automatic video-highlight editing without using helmet imagery or PUBG/KRAFTON artwork.

Adapted from Lucide `clapperboard` and `sparkle` at revision `1802f33f6c270395fcd981067f22719140e2ffbc`. The original paths are scaled, recolored, and combined with adjusted strokes, a geometric play symbol, and a new background. These Lucide icons are ISC-licensed. Full copyright/permission notices and source links are in root `THIRD_PARTY_NOTICES.md`, which is shipped with UI distributions. Do not treat these third-party assets as solely covered by the repository's GPL license.

- Source: `ui/public/icon.svg` (also the browser favicon).
- Tauri default window icon: `ui/src-tauri/icons/icon.png` (512 × 512 RGBA).
- Windows executable, installer and uninstaller: `ui/src-tauri/icons/icon.ico` (16, 20, 24, 32, 40, 48, 64, 128, 256 px).

## Regeneration

From the repository root in PowerShell, with Node.js, Python and Pillow available:

```powershell
$renderer = Join-Path $env:TEMP 'pht-icon-render'
npm.cmd install --prefix $renderer --no-audit --no-fund @resvg/resvg-js@2.6.2
$env:PHT_ICON_RENDERER = Join-Path $renderer 'node_modules/@resvg/resvg-js'
node -e "const {Resvg}=require(process.env.PHT_ICON_RENDERER); const fs=require('fs'); fs.writeFileSync('ui/src-tauri/icons/icon.png',new Resvg(fs.readFileSync('ui/public/icon.svg'),{fitTo:{mode:'width',value:512}}).render().asPng());"
python -c "from PIL import Image; im=Image.open('ui/src-tauri/icons/icon.png'); im.save('ui/src-tauri/icons/icon.ico',sizes=[(n,n) for n in (16,20,24,32,40,48,64,128,256)])"
```

These tools are needed only to regenerate artwork, not at application runtime. Rebuild the desktop application and installers to embed updated icons; existing binaries are not modified by regeneration.
