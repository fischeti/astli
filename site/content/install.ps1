# Installs the latest release of astli. Hands off to the installer the release
# itself carries, so this file needs no change from one release to the next.
$ErrorActionPreference = 'Stop'
Invoke-RestMethod https://github.com/fischeti/astli/releases/latest/download/astli-cli-installer.ps1 | Invoke-Expression
