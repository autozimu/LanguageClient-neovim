$version = '0.1.35'
$name = 'languageclient'
$url = "https://github.com/autozimu/LanguageClient-neovim/releases/download/$version/$name-$version-"

if ($ENV:PROCESSOR_ARCHITECTURE -eq 'AMD64') {
    $url += 'x86_64'
} else {
    $url += 'i686'
}

$url += '-pc-windows-gnu.exe'

$path = "bin\$name.exe"
if (Test-Path $path) {
    Remove-Item -Force $path
}
$downloader = New-Object System.Net.WebClient
echo "Downloading $url ..."
$downloader.DownloadFile($url, $path)
