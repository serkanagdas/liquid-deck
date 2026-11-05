param([Parameter(Mandatory=$true)][string]$SourceExe)
$ErrorActionPreference = 'Stop'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]$identity
if (!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Administrator approval is required for the read-only sensor helper.' }
if (!(Test-Path 'HKLM:\SYSTEM\CurrentControlSet\Services\PawnIO')) { throw 'PawnIO must already be installed. This setup never installs or changes drivers.' }
$source = (Resolve-Path -LiteralPath $SourceExe).Path
if ([IO.Path]::GetExtension($source) -ne '.exe') { throw 'Source must be the Liquid Deck executable.' }
# Elevation is restricted to an immutable helper copy under Program Files.
# Never create a highest-privilege task pointing at a user-writable executable.
$directory = Join-Path $env:ProgramFiles 'Liquid Deck Sensors'
$destination = Join-Path $directory 'liquid-deck-sensors.exe'
$taskName = 'Liquid Deck CPU Sensor'
if (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) { Stop-ScheduledTask -TaskName $taskName }
New-Item -ItemType Directory -Path $directory -Force | Out-Null
if ((Get-Item -LiteralPath $directory).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Sensor installation directory must not be a junction or symlink.' }
$acl = New-Object Security.AccessControl.DirectorySecurity
$acl.SetOwner([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))
$acl.SetAccessRuleProtection($true, $false)
$inheritance = [Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit'
$propagation = [Security.AccessControl.PropagationFlags]::None
foreach ($sid in @('S-1-5-18','S-1-5-32-544')) {
    $rule = New-Object Security.AccessControl.FileSystemAccessRule([Security.Principal.SecurityIdentifier]::new($sid), 'FullControl', $inheritance, $propagation, 'Allow')
    $acl.AddAccessRule($rule)
}
$read = New-Object Security.AccessControl.FileSystemAccessRule([Security.Principal.SecurityIdentifier]::new('S-1-5-32-545'), 'ReadAndExecute', $inheritance, $propagation, 'Allow')
$acl.AddAccessRule($read)
Set-Acl -LiteralPath $directory -AclObject $acl
if ((Test-Path $destination) -and ((Get-Item -LiteralPath $destination).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Sensor executable must not be a symlink.' }
Copy-Item -LiteralPath $source -Destination $destination -Force
# Reset ownership and ACL even when updating a pre-existing file.
$fileAcl = New-Object Security.AccessControl.FileSecurity
$fileAcl.SetOwner([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))
$fileAcl.SetAccessRuleProtection($true, $false)
foreach ($sid in @('S-1-5-18','S-1-5-32-544')) {
    $fileAcl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid), 'FullControl', 'Allow'))
}
$fileAcl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-5-32-545'), 'ReadAndExecute', 'Allow'))
Set-Acl -LiteralPath $destination -AclObject $fileAcl
$action = New-ScheduledTaskAction -Execute $destination -Argument '--cpu-sensor-helper' -WorkingDirectory $directory
$user = New-ScheduledTaskPrincipal -UserId $identity.Name -LogonType Interactive -RunLevel Highest
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew
# Demand-start only. The normal tray starts it after the HTTP listener is ready.
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $user -Settings $settings -Description 'Liquid Deck: read-only CPU temperature via existing PawnIO; no network server or fan control.' -Force | Out-Null
Start-ScheduledTask -TaskName $taskName
Write-Output "Installed read-only sensor helper: $destination"
