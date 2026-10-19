# Shared public-fixture preparation and output checks for CLI/GUI archive smoke.
# Do not print OCR text, subjects, or report contents to CI logs.
function New-PublicSmokeVideo([string]$Ffmpeg, [string]$Destination, [switch]$GuiLayout) {
    $fixture = Join-Path $PSScriptRoot '..\tests\fixtures\missed_event_ocr\23.20.15.09_2.png'
    if (-not (Test-Path -LiteralPath $fixture -PathType Leaf)) { throw "Required public OCR fixture is missing" }
    if ($GuiLayout) {
        # The WPF command uses the production default ROI. Embed the same public
        # crop there, matching the existing Rust end_to_end fixture placement.
        & $Ffmpeg -nostdin -hide_banner -loglevel error -f lavfi -i 'color=c=black:s=1920x1080:r=2:d=6' -loop 1 -framerate 2 -i $fixture -filter_complex "[0:v][1:v]overlay=x=576:y=712:enable='between(t,2,4)'" -t 6 -c:v libx264 -crf 0 -preset ultrafast -pix_fmt yuv444p -n $Destination 2>$null
    } else {
        & $Ffmpeg -nostdin -hide_banner -loglevel error -loop 1 -i $fixture -t 2 -vf 'pad=ceil(iw/2)*2:ceil(ih/2)*2' -r 10 -c:v libx264 -crf 0 -preset ultrafast -pix_fmt yuv444p -n $Destination 2>$null
    }
    if ($LASTEXITCODE -ne 0) { throw "Public-fixture synthetic video creation failed (exit $LASTEXITCODE)" }
}
function Assert-PublicSmokeResult([object]$Summary, [string]$Source, [string]$SourceHash, [string]$Ffmpeg, [string]$Ffprobe) {
    if ($Summary.source_count -ne 1 -or $Summary.included_count -ne 1 -or $Summary.unreadable_count -ne 0 -or $Summary.dry_run) { throw "Public OCR-to-trim smoke must include exactly one readable source" }
    if ($Summary.ocr_unavailable_reason) { throw "Public OCR-to-trim smoke reported unavailable OCR" }
    foreach ($field in @('methods','detectors','csv','candidate_events_csv','profile','merge_outputs','originals_moved','ocr_unavailable_reason')) {
        if ($null -eq $Summary.PSObject.Properties[$field]) { throw "Missing legacy summary field: $field" }
    }
    $rows = @(Import-Csv -LiteralPath $Summary.csv -Encoding UTF8)
    if ($rows.Count -ne 1 -or $rows[0].Status -ne 'included') { throw "Public OCR-to-trim CSV must have one included row" }
    foreach ($field in @('EventSec','KeepStartSec','KeepEndSec','PaddleText','Method','Detector','Output','Target','EventKeys')) {
        if ($null -eq $rows[0].PSObject.Properties[$field]) { throw "Missing legacy CSV field: $field" }
    }
    $clip = $rows[0].Output
    if (-not (Test-Path -LiteralPath $clip -PathType Leaf)) { throw "Public OCR-to-trim output media is missing" }
    $durationOutput = @(& $Ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 $clip 2>$null)
    if ($LASTEXITCODE -ne 0) { throw "Public OCR-to-trim output is not probe-readable" }
    $duration = [double]::Parse(($durationOutput -join '').Trim(), [Globalization.CultureInfo]::InvariantCulture)
    $expected = [double]::Parse($rows[0].KeepEndSec, [Globalization.CultureInfo]::InvariantCulture) - [double]::Parse($rows[0].KeepStartSec, [Globalization.CultureInfo]::InvariantCulture)
    if ($duration -le 0 -or [math]::Abs($duration - $expected) -gt 0.25) { throw "Public OCR-to-trim output duration differs from reported window" }
    & $Ffmpeg -nostdin -v error -i $clip -f null - 2>$null
    if ($LASTEXITCODE -ne 0) { throw "Public OCR-to-trim output cannot be decoded" }
    if ((Get-FileHash -LiteralPath $Source -Algorithm SHA256).Hash -ne $SourceHash) { throw "Public OCR-to-trim smoke modified the source video" }
}
