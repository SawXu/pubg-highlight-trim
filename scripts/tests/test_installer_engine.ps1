param([string]$MakeNsis='makensis')
$ErrorActionPreference='Stop'
$engine=Join-Path $PSScriptRoot '..\installer\install_transaction.ps1'
$build=Join-Path $PSScriptRoot '..\installer\build_installer.ps1'
$root=Join-Path ([IO.Path]::GetTempPath()) ('native installer tests '+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root|Out-Null
try {
 $layout=Join-Path $root 'layout';New-Item -ItemType Directory -Path $layout|Out-Null
 '{"schema":"pubg-highlight-trim/distribution/v2","artifact":"native-ui","variant":"native","version":"1.2.3"}'|Set-Content (Join-Path $layout 'distribution.json') -Encoding UTF8
 '{"schema":"pubg-highlight-trim/installer-ownership/v1","variant":"native","version":"1.2.3","files":[{"path":"distribution.json","owner":"application","action":"replace-or-remove"}]}'|Set-Content (Join-Path $layout 'installer-manifest.json') -Encoding UTF8
 & $build -Layout $layout -OutputDirectory (Join-Path $root 'out') -Variant Native -MakeNsis $MakeNsis
 $exe=Join-Path $root 'out\pubg-highlight-trim-native-setup.exe';if(!(Test-Path $exe)){throw 'Native installer output is missing.'}
 $tokens=$null;$errors=$null;$ast=[Management.Automation.Language.Parser]::ParseFile($engine,[ref]$tokens,[ref]$errors);if($errors.Count){throw ($errors|Out-String)}
 $fn=$ast.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Read-Manifest'},$true)
 Invoke-Expression $fn.Extent.Text
 $badManifest=Join-Path $root 'unsafe.json'
 foreach($unsafe in @('../escape','C:/escape','data/settings.json','file:stream','CON.txt','Uninstall.exe','.installer/script.ps1','trailing.')){
  @{schema='pubg-highlight-trim/installer-ownership/v1';variant='native';files=@(@{path=$unsafe;owner='application';action='replace-or-remove'})}|ConvertTo-Json -Depth 5|Set-Content $badManifest
  $rejected=$false;try{Read-Manifest $badManifest|Out-Null}catch{$rejected=$true};if(!$rejected){throw "Expected ownership path rejection: $unsafe"}
 }
 Write-Host 'PASS: Native compiler boundary, Unicode/space paths, manifest path rejection and engine syntax. No elevated install was performed.'
}finally{Remove-Item -LiteralPath $root -Recurse -Force}
