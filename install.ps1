#!/usr/bin/env pwsh

$version = '0.1.161'
$name = 'languageclient'
$url = "https://github.com/autozimu/LanguageClient-neovim/releases/download/$version/$name-$version-"

$path = "$PSScriptRoot\bin\$name"

switch ($true) {
    $IsMacOS {
        # MacOS is always x86_64
        $url += 'x86_64-apple-darwin'
    }
    $IsLinux {
        # Detecting architecture is more involved on Linux
        $arch = uname -sm
        $url += switch ($arch) {
            'Linux x86_64' { 'x86_64' }
            'Linux i686' { 'i686' }
            'Linux aarch64' { 'aarch64' }
            Default { throw 'architecture not supported' }
        }
        $url += '-unknown-linux-musl'
    }
    Default {
        # Windows
        $url += if ([Environment]::Is64BitOperatingSystem) { 'x86_64' } else { 'i686' }
        $url += '-pc-windows-gnu.exe'
        
        # We need to tack on the .exe to the end of the download path
        $path += '.exe'
    }
}

if (Test-Path -LiteralPath $path) {
    Remove-Item -Force -LiteralPath $path
}

echo "Downloading $url ..."

if (!$IsCoreCLR) {
    # We only need to do this for Windows PowerShell
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
}

Invoke-WebRequest -Uri $url -OutFile $path
