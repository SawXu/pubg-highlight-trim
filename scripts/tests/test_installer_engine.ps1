param([string]$MakeNsis='makensis')
$ErrorActionPreference='Stop'
$engine=Join-Path $PSScriptRoot '..\installer\install_transaction.ps1'
$build=Join-Path $PSScriptRoot '..\installer\build_installer.ps1'
$root=Join-Path ([IO.Path]::GetTempPath()) (([string][char]0x5b89+[char]0x88c5+[char]0x5668)+' tests '+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root|Out-Null
try {
 $layout=Join-Path $root 'layout';New-Item -ItemType Directory -Path $layout|Out-Null
 '{"schema":"pubg-highlight-trim/distribution/v1","variant":"system","minimum_webview2_major":120}'|Set-Content -LiteralPath (Join-Path $layout 'distribution.json') -Encoding UTF8
 '{"schema":"pubg-highlight-trim/installer-ownership/v1","variant":"system","files":[{"path":"distribution.json","owner":"application","action":"replace-or-remove"}]}'|Set-Content -LiteralPath (Join-Path $layout 'installer-manifest.json') -Encoding UTF8
 $bad=$false;try{& $build -Layout $layout -OutputDirectory (Join-Path $root 'out') -Variant Fixed -MakeNsis $MakeNsis}catch{if($_.Exception.Message -notlike '*does not match*'){throw};$bad=$true};if(!$bad){throw 'Expected variant rejection.'}
 & $build -Layout $layout -OutputDirectory (Join-Path $root 'out') -Variant System -MakeNsis $MakeNsis
 $exe=Join-Path $root 'out\pubg-highlight-trim-system-setup.exe';if(!(Test-Path $exe)){throw 'Installer output is missing.'}
 $tokens=$null;$errors=$null;$ast=[Management.Automation.Language.Parser]::ParseFile($engine,[ref]$tokens,[ref]$errors);if($errors.Count){throw ($errors|Out-String)}
 # Exercise the real manifest parser without bypassing the shipping engine's
 # elevation check or modifying ACLs on the development host.
 $fn=$ast.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Read-Manifest'},$true)
 Invoke-Expression $fn.Extent.Text
 $badManifest=Join-Path $root 'unsafe.json'
 foreach($unsafe in @('../escape','C:/escape','data/settings.json','file:stream','CON.txt','Uninstall.exe','.installer/script.ps1','trailing.')){
  @{schema='pubg-highlight-trim/installer-ownership/v1';variant='system';files=@(@{path=$unsafe;owner='application';action='replace-or-remove'})}|ConvertTo-Json -Depth 5|Set-Content -LiteralPath $badManifest
  $rejected=$false;try{Read-Manifest $badManifest|Out-Null}catch{$rejected=$true};if(!$rejected){throw "Expected ownership path rejection: $unsafe"}
 }
 Write-Host 'PASS: compiler boundary, variant rejection, Unicode/space paths, manifest path rejection and engine syntax. No elevated install was performed.'
}finally{Remove-Item -LiteralPath $root -Recurse -Force}
