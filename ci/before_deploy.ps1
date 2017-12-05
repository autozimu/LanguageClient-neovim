$name = languageclient

$SRC_DIR = $PWD.Path
$filename = "$SRC_DIR\target\release\$($Env:CRATE_NAME)-$($Env:APPVEYOR_REPO_TAG_NAME)-$($Env:TARGET).exe"

Copy-Item "$SRC_DIR\target\release\$name.exe" $filename

Push-AppveyorArtifact "$filename"
