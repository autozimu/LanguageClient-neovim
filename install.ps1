$version = '0.1.32'
$name = 'languageclient'
$url = "https://github.com/autozimu/LanguageClient-neovim/releases/download/$version/"
$filename = "$name-$version-"

if ($ENV:PROCESSOR_ARCHITECTURE -eq 'AMD64') {
    $filename += 'x86_64'
} else {
    $filename += 'i686'
}

$filename += '-pc-windows-gnu.exe'
$url += $filename

$path = "bin\$name.exe"
if (Test-Path $path) {
    Remove-Item -Force $path
}
$downloader = New-Object System.Net.WebClient
echo "Downloading $url ..."
$downloader.DownloadFile($url, $path)
