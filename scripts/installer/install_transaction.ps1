[CmdletBinding()]
param(
 [Parameter(Mandatory)][ValidateSet('Install','Uninstall')][string]$Operation,
 [Parameter(Mandatory)][string]$Layout,
 [Parameter(Mandatory)][string]$Target,
 [switch]$DeleteData
)
$ErrorActionPreference='Stop'
$registry='HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\pubg-highlight-trim'
function Assert-Local([string]$Path) {
 if($Path -notmatch '^[A-Za-z]:[\\/]' -or $Path -match '[<>"|?*\x00-\x1f]' -or $Path.Substring(2).Contains(':')){throw 'An absolute local drive path is required.'}
 foreach($part in ($Path.Substring(3) -split '[\\/]')){if($part -match '[. ]$' -or $part -match '^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])([.]|$)'){throw 'Ambiguous, relative or reserved path components are not supported.'}}
 $p=[IO.Path]::GetFullPath($Path).TrimEnd('\')
 $drive=New-Object IO.DriveInfo ($p.Substring(0,3));if($drive.DriveType -ne 'Fixed'){throw 'Only local fixed disks are supported.'}
 if($p.Length -lt 4){throw 'A drive root cannot be an installation directory.'}
 $cursor=$p;while($cursor){if(Test-Path -LiteralPath $cursor){if((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Reparse point is not supported: $cursor"}};$cursor=Split-Path $cursor -Parent}
 return $p
}
function Assert-Tree([string]$p){if(Test-Path -LiteralPath $p){foreach($i in @(Get-Item -LiteralPath $p -Force)+@(Get-ChildItem -LiteralPath $p -Recurse -Force)){if($i.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Reparse point is not supported: $($i.FullName)"}}}}
function Read-Manifest([string]$p){
 $m=Get-Content -LiteralPath $p -Raw|ConvertFrom-Json
 if($m.schema -ne 'pubg-highlight-trim/installer-ownership/v1' -or $m.variant -notin @('system','fixed')){throw 'Invalid ownership manifest.'}
 $seen=@{};foreach($f in $m.files){$s=[string]$f.path;if($f.owner -ne 'application' -or $f.action -ne 'replace-or-remove' -or !$s -or $s -match '[<>:"|?*\x00-\x1f]' -or $s -match '(^|[\\/])(\.|\.\.|data|\.installer)([\\/]|$)' -or [IO.Path]::IsPathRooted($s) -or $s -match '[. ]([\\/]|$)'){throw "Unsafe ownership path: $s"};if($s -match '(^|[\\/])(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])([.]|[\\/]|$)' -or $s -ieq 'Uninstall.exe'){throw "Reserved ownership path: $s"};$key=$s.Replace('/','\').ToLowerInvariant();if($seen.ContainsKey($key)){throw 'Duplicate ownership path.'};$seen[$key]=$true}
 if(!$seen.Count){throw 'Empty ownership manifest.'};return $m
}
function Paths($m){@($m.files|ForEach-Object{$_.path.Replace('/','\')})}
function Assert-ProtectedCodeDirectory([string]$path){
 $a=Get-Acl -LiteralPath $path
 if($a.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne 'S-1-5-32-544' -or !$a.AreAccessRulesProtected){throw "Installation directory is not administrator-owned and protected: $path"}
 foreach($r in $a.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])){if($r.AccessControlType -ne 'Allow' -and $r.IdentityReference.Value -notin @('S-1-5-18','S-1-5-32-544')){throw "Unsafe deny ACL on installation directory: $path"};if($r.IdentityReference.Value -notin @('S-1-5-18','S-1-5-32-544')){ $allowed=[long]0x1200A9; if(([long]$r.FileSystemRights -band (-bnot $allowed)) -ne 0){throw "Unsafe writable ACL on installation directory: $path"} }}
}
function Acl([string]$path,[string]$user,[switch]$Data,[switch]$Runtime){
 $a=New-Object Security.AccessControl.DirectorySecurity;$inherit='None'
 # Only a newly created empty data root may propagate ACLs to future files.
 if($Data){$inherit='ContainerInherit,ObjectInherit'}
 if(!(Get-Item -LiteralPath $path).PSIsContainer){$a=New-Object Security.AccessControl.FileSecurity;$inherit='None'}
 $a.SetAccessRuleProtection($true,$false);$a.SetOwner((New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')))
 foreach($sid in @('S-1-5-18','S-1-5-32-544')){$a.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier($sid)),'FullControl',$inherit,'None','Allow')))}
 $right='ReadAndExecute';if($Data){$right='Modify'}
 $a.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier($user)),$right,$inherit,'None','Allow')))
 if($Runtime){foreach($sid in @('S-1-15-2-1','S-1-15-2-2')){$a.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier($sid)),'ReadAndExecute',$inherit,'None','Allow')))}}
 Set-Acl -LiteralPath $path -AclObject $a
}
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
if(-not ([Security.Principal.WindowsPrincipal]$identity).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator privileges are required.'}
. (Join-Path $PSScriptRoot 'DirectoryGuard.ps1')
$guards=New-Object 'Collections.Generic.List[IDisposable]'
$createdDirectories=New-Object 'Collections.Generic.List[string]'
$transactionMutex=New-Object Threading.Mutex($false,'Global\PubgHighlightTrimInstaller')
$mutexHeld=$false
try {
try{$mutexHeld=$transactionMutex.WaitOne(0)}catch [Threading.AbandonedMutexException] {throw 'An earlier installer was interrupted. Inspect the recovery directory before retrying.'}
if(!$mutexHeld){throw 'Another installer or uninstaller is running.'}
$Target=Assert-Local $Target;$Layout=Assert-Local $Layout
# Pin parents first so an empty, unregistered user-created target can be
# replaced by a fresh protected object rather than having its ACL taken over.
Add-InstallerDirectoryPins -Path (Split-Path $Target -Parent) -Handles $guards
Add-InstallerDirectoryPins -Path $Layout -Handles $guards
# Parent chains are pinned above; their ownership and ACL are never changed.
function Ensure-ProtectedDirectory([string]$Path,[switch]$SkipValidation){
 $parent=Split-Path $Path -Parent
 if($Path -ine $Target -and $Path.StartsWith($Target+'\',[StringComparison]::OrdinalIgnoreCase)){
  # Validate every code ancestor, not only a leaf below an unowned writable directory.
  Ensure-ProtectedDirectory $parent
 }
 if(Test-Path -LiteralPath $Path){
  $guards.Add([PhtDirectoryGuard]::Pin($Path))
  # Data is intentionally user-writable; parent chains are only pinned. Code
  # and control directories must already be administrator-owned/protected.
  if(!$SkipValidation -and $Path -ine (Join-Path $Target 'data') -and !$Path.StartsWith((Join-Path $Target 'data')+'\',[StringComparison]::OrdinalIgnoreCase)){Assert-ProtectedCodeDirectory -path $Path}
 }else{
  $parent=Split-Path $Path -Parent
  if(!(Test-Path -LiteralPath $parent)){Ensure-ProtectedDirectory $parent -SkipValidation}
  [PhtDirectoryGuard]::CreateProtected($Path)
  $createdDirectories.Add($Path)
  $guards.Add([PhtDirectoryGuard]::Pin($Path))
  Assert-ProtectedCodeDirectory -path $Path
 }
}
$registered=Get-ItemProperty $registry -ErrorAction SilentlyContinue
if($registered -and $registered.InstallLocation -ine $Target){throw 'Select the existing registered installation directory.'}
if($registered -and !(Test-Path -LiteralPath $Target -PathType Container)){throw 'Registered installation target is missing; refusing to create a replacement.'}
if($Operation -eq 'Uninstall' -and !$registered){throw 'No registered installation exists; nothing was changed.'}
if($Operation -eq 'Install' -and !$registered -and (Test-Path -LiteralPath $Target -PathType Container)){
 $targetPin=[PhtDirectoryGuard]::Pin($Target)
 try{
  $trustedTarget=$true
  try{Assert-ProtectedCodeDirectory $Target}catch{$trustedTarget=$false}
  if(!$trustedTarget){
   if(@([IO.Directory]::EnumerateFileSystemEntries($Target)).Count){throw 'The selected directory is not empty and is not a protected application directory. Select a new application subdirectory; no existing files or parent permissions will be changed.'}
   # Do not re-ACL an unsafe directory: stale WRITE_DAC handles would survive.
   # Deleting only an empty object and atomically creating a new protected one
   # means old handles can never acquire rights on the new object.
   $targetPin.Dispose()
   [IO.Directory]::Delete($Target,$false)
  }
 }finally{$targetPin.Dispose()}
}
Ensure-ProtectedDirectory $Target
Assert-ProtectedCodeDirectory $Target
# Store only a one-way binding digest, never raw volume/file identifiers.
$identityHasher=[Security.Cryptography.SHA256]::Create()
try{$targetIdentity=([BitConverter]::ToString($identityHasher.ComputeHash([Text.Encoding]::UTF8.GetBytes([PhtDirectoryGuard]::Identity($guards[$guards.Count-1]))))).Replace('-','')}finally{$identityHasher.Dispose()}
if($Layout -ieq $Target -or $Layout.StartsWith($Target+'\',[StringComparison]::OrdinalIgnoreCase) -or $Target.StartsWith($Layout+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Payload and destination must not overlap.'}
Assert-Tree $Target;Assert-Tree $Layout
# Data belongs to the unique Explorer desktop owner in this session, not the
# administrator account whose credentials were used for UAC elevation.
$session=(Get-Process -Id $PID).SessionId
$explorers=@(Get-CimInstance Win32_Process -Filter "Name='explorer.exe'"|Where-Object{$_.SessionId -eq $session})
$users=@($explorers|ForEach-Object{(Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid).Sid}|Select-Object -Unique)
if($users.Count -ne 1 -or [string]::IsNullOrWhiteSpace($users[0])){throw 'Cannot identify a unique desktop user for this session. Start the installer from the intended user desktop.'}
$user=$users[0]
if($registered -and $registered.InstallLocation -ine $Target){throw "This application is already installed at $($registered.InstallLocation). Select the existing directory."}
if($registered -and (!$registered.TargetIdentity -or $registered.TargetIdentity -ne $targetIdentity)){throw 'Registered installation target identity mismatch.'}
$meta=Join-Path $Target '.installer';$oldPath=Join-Path $meta 'installer-manifest.json';$old=$null
if(Test-Path -LiteralPath $meta){Ensure-ProtectedDirectory $meta}
# Never trust an editable on-disk ownership list without the corresponding
# administrator-protected registration and digest from the last commit.
if($registered){
 if(!(Test-Path -LiteralPath $oldPath -PathType Leaf) -or !$registered.OwnershipSha256){throw 'Registered installation ownership metadata is missing.'}
 $actual=(Get-FileHash -LiteralPath $oldPath -Algorithm SHA256).Hash
 if($actual -ine $registered.OwnershipSha256){throw 'Installed ownership manifest digest mismatch. Refusing destructive operations.'}
 $old=Read-Manifest $oldPath
}else{
 if((Test-Path -LiteralPath $meta) -or (Test-Path -LiteralPath (Join-Path $Target 'Uninstall.exe'))){throw 'Unregistered installer control files exist. Refusing to adopt their ownership claims.'}
}
if($Operation -eq 'Uninstall' -and !$old){throw 'No registered installation ownership manifest exists.'}
$new=$null;if($Operation -eq 'Install'){
 $new=Read-Manifest (Join-Path $Layout 'installer-manifest.json');$d=Get-Content (Join-Path $Layout 'distribution.json') -Raw|ConvertFrom-Json
 if($d.schema -ne 'pubg-highlight-trim/distribution/v1' -or $d.variant -ne $new.variant -or $d.minimum_webview2_major -ne 120 -or $d.version -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' -or $new.version -ne $d.version){throw 'Distribution metadata mismatch.'}
 if($new.variant -eq 'fixed'){foreach($name in @('msedgewebview2.exe','msedge.dll','icudtl.dat','resources.pak')){if(!(Test-Path -LiteralPath (Join-Path $Layout "WebView2Runtime\$name") -PathType Leaf)){throw "Fixed runtime missing: $name"}}}
 # Alternate credentials change HKCU (runtime discovery and loader policies).
 # Until a target-user token probe exists, do not destroy a working Fixed copy
 # on the strength of the administrator's unrelated runtime environment.
 if($new.variant -eq 'system' -and $identity.User.Value -ne $user){
  throw 'System installation requires same-account UAC elevation for runtime preflight. With alternate administrator credentials choose Fixed; the existing installation is preserved.'
 }
 # The payload UI has a no-window, no-data native loader probe. Run it before
 # moving any old owned file; registry presence alone is insufficient.
 $probe=Join-Path $Layout 'pubg-highlight-trim-ui.exe'
 if(!(Test-Path -LiteralPath $probe -PathType Leaf)){throw 'Runtime probe executable is missing.'}
 $probeProcess=Start-Process -FilePath $probe -ArgumentList @('--check-webview2-runtime',$new.variant) -WorkingDirectory $Layout -Wait -PassThru
 if($probeProcess.ExitCode -ne 0){throw "Compatible $($new.variant) WebView2 runtime is unavailable. Existing installation was not modified. For a missing system runtime, choose the Fixed installer."}
 if($old -and $old.variant -ne $new.variant){Add-Type -AssemblyName System.Windows.Forms;if([Windows.Forms.MessageBox]::Show("Switch from $($old.variant) to $($new.variant)? Settings are preserved.",'Confirm variant switch','YesNo','Warning') -ne 'Yes'){throw 'User cancelled variant switch.'}}
}
# Deliberately do not delete browser/cache trees: they may contain unowned files.
# Only exact UI-owned settings/log filenames are eligible for explicit deletion.
$dataFiles=@('settings.json','settings.tmp','run.log')
if($DeleteData -and $Operation -ne 'Uninstall'){throw 'Data deletion is only available during uninstall.'}
$all=@();if($old){$all+=Paths $old};if($new){$all+=Paths $new};$all=@($all|Select-Object -Unique)
$control=@('.installer\installer-manifest.json','.installer\install_transaction.ps1','Uninstall.exe')
$all=@($all+$control|Select-Object -Unique)
# Protect unknown user files from being overwritten, including first install.
$newFiles=@();if($new){$newFiles=@(Paths $new)+$control}
$oldFiles=@();if($old){$oldFiles=@(Paths $old)+$control}
foreach($p in $newFiles){if((Test-Path -LiteralPath (Join-Path $Target $p)) -and $p -notin $oldFiles){throw "Refusing to overwrite an unowned file: $p"}}
# Pin every preexisting owned directory before using its file paths. The data
# directory is writable by design, so pin it without granting or repairing ACLs.
foreach($p in $all){
 $dir=Split-Path (Join-Path $Target $p) -Parent
 Ensure-ProtectedDirectory $dir
}
$dataRoot=Join-Path $Target 'data'
if(Test-Path -LiteralPath $dataRoot){$guards.Add([PhtDirectoryGuard]::Pin($dataRoot))}
$parent=Split-Path $Target -Parent
$work=Join-Path $parent ('.pht-transaction-'+[guid]::NewGuid().ToString('N'));$stage=Join-Path $work 'stage';$backup=Join-Path $work 'backup'
[PhtDirectoryGuard]::CreateProtected($work)
$workPin=[PhtDirectoryGuard]::Pin($work)
$guards.Add($workPin)
Assert-ProtectedCodeDirectory -path $work
New-Item -ItemType Directory -Path $stage,$backup -Force|Out-Null
$moved=New-Object 'Collections.Generic.List[string]';$placed=New-Object 'Collections.Generic.List[string]';$aclJournal=@{};$createdData=$false;$committed=$false
$oldRegistry=@{};if($registered){foreach($prop in $registered.PSObject.Properties){if($prop.Name -notlike 'PS*'){$oldRegistry[$prop.Name]=$prop.Value}}}
try{
 # Snapshot preexisting ACLs before replacement; restored old files must not
 # receive the newly staged file ACL during rollback.
 foreach($p in $all){$f=Join-Path $Target $p;$dir=Split-Path $f -Parent;while($dir -and $dir.StartsWith($Target,[StringComparison]::OrdinalIgnoreCase)){if(Test-Path -LiteralPath $dir){$aclJournal[$dir]=Get-Acl -LiteralPath $dir};if($dir -ieq $Target){break};$dir=Split-Path $dir -Parent}}
 if($new){
  foreach($p in (Paths $new)){ $src=Join-Path $Layout $p;if(!(Test-Path -LiteralPath $src -PathType Leaf)){throw "Missing payload: $p"};$dst=Join-Path $stage $p;New-Item -ItemType Directory -Path (Split-Path $dst) -Force|Out-Null;Copy-Item -LiteralPath $src -Destination $dst }
  New-Item -ItemType Directory -Path (Join-Path $stage '.installer') -Force|Out-Null
  Copy-Item -LiteralPath (Join-Path $Layout 'installer-manifest.json') -Destination (Join-Path $stage '.installer\installer-manifest.json')
  Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $stage '.installer\install_transaction.ps1')
  Copy-Item -LiteralPath (Join-Path $Layout 'Uninstall.exe') -Destination (Join-Path $stage 'Uninstall.exe')
 }
 # Acquire exclusive handles for every existing owned file before moving any.
 $locks=@();try{foreach($p in $all){$f=Join-Path $Target $p;if(Test-Path -LiteralPath $f -PathType Leaf){$locks+= [IO.File]::Open($f,'Open','ReadWrite','None')}}}finally{foreach($h in $locks){$h.Dispose()}}
 if($new){
  # Apply final ACLs while files are private and newly created; never Set-Acl on
  # an old destination file (which could be a hard link to an outside object).
  $stageCode=@($stage)
  foreach($p in $newFiles){$f=Join-Path $stage $p;$stageCode+=$f;$dir=Split-Path $f -Parent;while($dir -ine $stage){$stageCode+=$dir;$dir=Split-Path $dir -Parent}}
  foreach($f in @($stageCode|Select-Object -Unique)){Acl $f $user -Runtime:($f -ieq (Join-Path $stage 'WebView2Runtime') -or $f.StartsWith((Join-Path $stage 'WebView2Runtime')+'\',[StringComparison]::OrdinalIgnoreCase))}
 }
 # Finish preparing and protecting the new payload before moving any old file.
 foreach($p in $all){$f=Join-Path $Target $p;if(Test-Path -LiteralPath $f -PathType Leaf){$b=Join-Path $backup $p;New-Item -ItemType Directory -Path (Split-Path $b) -Force|Out-Null;Move-Item -LiteralPath $f -Destination $b;$moved.Add($p)}}
 if($new){
  foreach($p in $newFiles){$f=Join-Path $stage $p;$dst=Join-Path $Target $p;New-Item -ItemType Directory -Path (Split-Path $dst) -Force|Out-Null;Move-Item -LiteralPath $f -Destination $dst;$placed.Add($p)}
  $code=@($Target)
  foreach($p in $newFiles){$f=Join-Path $Target $p;$code+=$f;$dir=Split-Path $f -Parent;while($dir -ine $Target){$code+=$dir;$dir=Split-Path $dir -Parent}}
  foreach($f in @($code|Select-Object -Unique)){
   if(Test-Path -LiteralPath $f -PathType Container){
    if(!$aclJournal.ContainsKey($f)){$aclJournal[$f]=Get-Acl -LiteralPath $f}
    # Final code/control directories receive only administrator/SYSTEM and
    # user RX; existing files are never ACL-touched (hard-link safe).
    Acl $f $user -Runtime:($f -ieq (Join-Path $Target 'WebView2Runtime') -or $f.StartsWith((Join-Path $Target 'WebView2Runtime')+'\',[StringComparison]::OrdinalIgnoreCase))
   }
  }
  $data=Join-Path $Target 'data'
  if(!(Test-Path -LiteralPath $data)){$createdData=$true;New-Item -ItemType Directory -Path $data|Out-Null;Acl $data $user -Data}else{
   # Never ACL descendants of a writable directory: hard-linked files can
   # reference objects outside the install tree without being reparse points.
   $dataAcl=Get-Acl -LiteralPath $data
   $owner=$dataAcl.GetOwner([Security.Principal.SecurityIdentifier]).Value
   if($owner -ne 'S-1-5-32-544' -or !$dataAcl.AreAccessRulesProtected){throw 'Existing data directory has unexpected ownership or inherited permissions.'}
   $allowed=@('S-1-5-18','S-1-5-32-544',$user)
   $rules=@($dataAcl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]))
   foreach($rule in $rules){if($rule.IdentityReference.Value -notin $allowed -or $rule.AccessControlType -ne 'Allow'){throw 'Existing data ACL contains unexpected principals or deny rules.'}}
   foreach($sid in $allowed){if(!@($rules|Where-Object{$_.IdentityReference.Value -eq $sid -and ($_.FileSystemRights -band [Security.AccessControl.FileSystemRights]::Modify) -eq [Security.AccessControl.FileSystemRights]::Modify}).Count){throw 'Existing data ACL is missing a required principal or permission.'}}
  }
  New-Item -Path $registry -Force|Out-Null
  foreach($entry in @{DisplayName='PUBG Highlight Trim';InstallLocation=$Target;DisplayVersion=$d.version;UninstallString=('"'+(Join-Path $Target 'Uninstall.exe')+'"');Variant=$new.variant;TargetIdentity=$targetIdentity;OwnershipSha256=(Get-FileHash -LiteralPath (Join-Path $meta 'installer-manifest.json') -Algorithm SHA256).Hash}.GetEnumerator()){New-ItemProperty -Path $registry -Name $entry.Key -Value $entry.Value -PropertyType String -Force|Out-Null}
 }else{
  if($DeleteData){foreach($p in $dataFiles){$f=Join-Path (Join-Path $Target 'data') $p;if(Test-Path -LiteralPath $f -PathType Leaf){$b=Join-Path $backup ('data\'+$p);New-Item -ItemType Directory -Path (Split-Path $b) -Force|Out-Null;[IO.File]::Move($f,$b);$moved.Add('data\'+$p)}}}
  Remove-Item -LiteralPath $registry -ErrorAction SilentlyContinue
 }
 $committed=$true
}catch{
 $failure=$_;$rollbackError=$null
 try{foreach($p in $placed){Remove-Item -LiteralPath (Join-Path $Target $p) -Force};foreach($p in $moved){$f=Join-Path $Target $p;New-Item -ItemType Directory -Path (Split-Path $f) -Force|Out-Null;Move-Item -LiteralPath (Join-Path $backup $p) -Destination $f};foreach($dir in $aclJournal.Keys){if(Test-Path -LiteralPath $dir){Set-Acl -LiteralPath $dir -AclObject $aclJournal[$dir]}};if($createdData){Remove-Item -LiteralPath (Join-Path $Target 'data')};if($registered){New-Item $registry -Force|Out-Null;foreach($k in $oldRegistry.Keys){New-ItemProperty $registry -Name $k -Value $oldRegistry[$k] -PropertyType String -Force|Out-Null}}else{Remove-Item $registry -ErrorAction SilentlyContinue}}catch{$rollbackError=$_}
 if($rollbackError){throw "Transaction failed: $failure. Recovery failed: $rollbackError. DO NOT DELETE recovery files: $work"};throw $failure
}finally{
 if($committed -or !$rollbackError){
  # Remove protected transaction children while the work pin is held, then
  # release that pin and delete only the now-empty work root.
  try {
   if(Test-Path -LiteralPath $stage){[IO.Directory]::Delete($stage,$true)}
   if(Test-Path -LiteralPath $backup){[IO.Directory]::Delete($backup,$true)}
   $workPin.Dispose();[void]$guards.Remove($workPin)
   [IO.Directory]::Delete($work,$false)
  } catch { Write-Warning "Private transaction files retained at $work : $_" }
 }
}
} finally {
 foreach($handle in $guards){$handle.Dispose()}
 # Remove only empty directories created by this attempt. In particular, an
 # aborted first install must not leave empty control metadata that blocks retry.
 if(!$committed){for($i=$createdDirectories.Count-1;$i -ge 0;$i--){try{[IO.Directory]::Delete($createdDirectories[$i],$false)}catch{}}}
 if($committed -and $Operation -eq 'Uninstall'){
  # Empty control metadata must not block a later reinstall with retained data.
  # Nonempty metadata remains untouched and will deliberately require inspection.
  try{[IO.Directory]::Delete($meta,$false)}catch{}
 }
 if($mutexHeld){$transactionMutex.ReleaseMutex()}
 $transactionMutex.Dispose()
}
