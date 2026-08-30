//! Build the PowerShell used to retarget Windows .lnk icons (unit-tested).

use std::path::Path;

fn powershell_escape(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

pub(crate) fn shortcut_update_script(exe: &Path, ico: &Path) -> String {
    let exe_s = powershell_escape(exe);
    let ico_s = powershell_escape(ico);
    format!(
        r#"
$ErrorActionPreference = 'Stop'
$exe = [IO.Path]::GetFullPath('{exe_s}')
$ico = [IO.Path]::GetFullPath('{ico_s}')
$shell = New-Object -ComObject WScript.Shell
$roots = @(
  [Environment]::GetFolderPath('Desktop'),
  [Environment]::GetFolderPath('CommonDesktopDirectory'),
  Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs',
  Join-Path ([Environment]::GetFolderPath('CommonStartMenu')) 'Programs'
)
foreach ($root in $roots) {{
  if (-not $root -or -not (Test-Path -LiteralPath $root)) {{ continue }}
  Get-ChildItem -LiteralPath $root -Filter *.lnk -Recurse -ErrorAction SilentlyContinue | ForEach-Object {{
    try {{
      $s = $shell.CreateShortcut($_.FullName)
      if (-not $s.TargetPath) {{ return }}
      $target = [IO.Path]::GetFullPath($s.TargetPath)
      if ($target -ieq $exe) {{
        $s.IconLocation = "$ico,0"
        $s.Save()
      }}
    }} catch {{}}
  }}
}}
"#
    )
}
