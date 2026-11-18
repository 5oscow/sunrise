# PURPOSE: proves packaging/server/install.ps1 installs only what sunrise-verify has checked against the release's
# signature, and refuses everything else with a message naming the cause (docs/release-flow.md, "Signed checksums").
# WHY: the first install on Windows used to check the signature only when OpenSSL 3 was installed (docs/security.md).
# REFS: tests/install_sh_signature.sh (the same cases for install.sh), tools/ci/sign_release.sh (the format signed here),
# crates/sunrise-release-sign/tests/install_ps1.rs (the daily run's driver, which builds sunrise-verify with test-key).
# LOGS: prints one ok/FAIL line per case; exit 1 when any case fails.
# READS-BY: crates/sunrise-release-sign/tests/install_ps1.rs (daily, Windows), tests/test_install_ps1_signature.py
# (skips unless SUNRISE_VERIFY_TEST_BIN is set), by hand with SUNRISE_VERIFY_TEST_BIN=<sunrise-verify.exe built with
# --features test-key>: pwsh -File tests/install_ps1_signature.ps1
# ENDS-WHEN: it ends by itself; leaves nothing outside its own temporary folder.
# Windows only, Windows PowerShell 5.1 or PowerShell 7. Needs OpenSSL 3 (to sign the throwaway releases; install.ps1
# itself uses none) and SUNRISE_VERIFY_TEST_BIN; skips (exit 77) without them. Invoke-RestMethod and Invoke-WebRequest are replaced by fakes that serve local files, and the
# user's PATH is never written, so nothing touches the network or the account.
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$Root = Split-Path -Parent $PSScriptRoot
$Installer = Join-Path $Root "packaging\server\install.ps1"

function Find-TestOpenSsl {
    $Paths = @()
    $OnPath = Get-Command openssl -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($OnPath) { $Paths += $OnPath.Source }
    $Git = Get-Command git -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($Git) { $Paths += Join-Path (Split-Path (Split-Path $Git.Source)) "usr\bin\openssl.exe" }
    foreach ($Path in $Paths) {
        if (-not (Test-Path -LiteralPath $Path)) { continue }
        if ("$(& $Path version 2>$null)" -match "^OpenSSL [3-9]\.") { return $Path }
    }
    return $null
}
$Ssl = Find-TestOpenSsl
if (-not $Ssl) { Write-Output "skip: OpenSSL 3 needed"; exit 77 }
$VerifyBin = $env:SUNRISE_VERIFY_TEST_BIN
if (-not $VerifyBin -or -not (Test-Path -LiteralPath $VerifyBin)) {
    Write-Output "skip: SUNRISE_VERIFY_TEST_BIN (a sunrise-verify.exe built with --features test-key) needed"; exit 77
}
$VerifySha = (Get-FileHash -Algorithm SHA256 -LiteralPath $VerifyBin).Hash.ToLowerInvariant()

$W = Join-Path ([IO.Path]::GetTempPath()) ("sunrise-ps1-test-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $W | Out-Null
# install.ps1 honours the tests' pin overrides only for a -Dir inside this root.
$env:SUNRISE_INSTALL_TEST_ROOT = $W
$env:SUNRISE_INSTALL_TEST_VERIFIER_VERSION = "test"
# The installer always picks the x64 archive, the one these releases hold, whatever this machine is.
$env:PROCESSOR_ARCHITECTURE = "AMD64"
$env:PROCESSOR_ARCHITEW6432 = $null
$SavedPath = $env:PATH
$Asset = "sunrise-windows-x64.zip"
$VerifierAsset = "sunrise-verify-windows-x64.exe"
$script:Fails = 0
$global:SunriseTestRel = $null

# The user's PATH stays as it is: the copies of install.ps1 write through this class instead of [Environment].
Add-Type -TypeDefinition @"
public static class FakeEnvironment {
    public static string GetEnvironmentVariable(string name, string scope) { return ""; }
    public static void SetEnvironmentVariable(string name, string value, string scope) { }
}
"@

# The fakes: the API answers with the case's release, any download with the case's file of that name.
function Invoke-RestMethod { param($Uri, $Headers) $global:SunriseTestRel.Release }
function Invoke-WebRequest {
    param($Uri, $OutFile, [switch]$UseBasicParsing)
    $Source = Join-Path $global:SunriseTestRel.Files ($Uri -split "/")[-1]
    if (-not (Test-Path -LiteralPath $Source)) { throw "404 $Uri" }
    Copy-Item -LiteralPath $Source -Destination $OutFile
}

function Write-Ascii([string]$Path, [string]$Text) { [IO.File]::WriteAllText($Path, $Text, [Text.Encoding]::ASCII) }
function Get-Sha([string]$Path) { (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant() }
function Get-Title([string]$Tag, [string]$Also) {
    if ($Also) { "Nightly $($Also.Substring(8, 10)) $([char]0xB7) $($Also.Substring(19))" } else { $Tag }
}

# One base64 signature line over "horizon release <tag>\n" + SHA256SUMS, as tools/ci/sign_release.sh writes it.
function New-SignatureLine([string]$Dir, [string]$Tag) {
    $Message = Join-Path $Dir "message"
    [IO.File]::WriteAllBytes($Message, [byte[]]([Text.Encoding]::ASCII.GetBytes("horizon release $Tag`n") + [IO.File]::ReadAllBytes((Join-Path $Dir "files\SHA256SUMS"))))
    & $Ssl pkeyutl -sign -rawin -inkey (Join-Path $Dir "key") -keyform DER -in $Message -out (Join-Path $Dir "sig.bin")
    [Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $Dir "sig.bin"))) + "`n"
}

# A fake release: a zip holding sunrise-server.exe, signed for Tag with a throwaway key. A Nightly (Tag nightly) is
# also signed for its own version Also (nightly-<date>-<commit>) in SHA256SUMS.v2.sig, titled as nightly-build.yml
# titles it.
function New-Release([string]$Dir, [string]$Tag, [string]$Also = "") {
    New-Item -ItemType Directory -Force -Path (Join-Path $Dir "src\sunrise-server-x"), (Join-Path $Dir "files") | Out-Null
    Write-Ascii (Join-Path $Dir "src\sunrise-server-x\sunrise-server.exe") "fake"
    Compress-Archive -Path (Join-Path $Dir "src\sunrise-server-x") -DestinationPath (Join-Path $Dir "files\$Asset")
    Write-Ascii (Join-Path $Dir "files\SHA256SUMS") "$(Get-Sha (Join-Path $Dir "files\$Asset"))  $Asset`n"
    & $Ssl genpkey -algorithm ed25519 -outform DER -out (Join-Path $Dir "key")
    & $Ssl pkey -inform DER -in (Join-Path $Dir "key") -pubout -outform DER -out (Join-Path $Dir "pub.der")
    $Pub = [IO.File]::ReadAllBytes((Join-Path $Dir "pub.der"))
    $script:Keys[$Dir] = [Convert]::ToBase64String($Pub[($Pub.Length - 32)..($Pub.Length - 1)])
    Copy-Item -LiteralPath $VerifyBin -Destination (Join-Path $Dir "files\$VerifierAsset")
    Write-Ascii (Join-Path $Dir "files\SHA256SUMS.sig") (New-SignatureLine $Dir $Tag)
    if ($Also) { Write-Ascii (Join-Path $Dir "files\SHA256SUMS.v2.sig") (New-SignatureLine $Dir $Also) }
    Set-Release $Dir $Tag $true (Get-Title $Tag $Also)
    Set-Installer $Dir
}
$script:Keys = @{}

# GitHub's release, as Invoke-RestMethod returns it: the archive with its digest, and the checksum files.
function Set-Release([string]$Dir, [string]$Tag, [bool]$Sums, [string]$Title = $Tag) {
    $Assets = @([pscustomobject]@{
        name = $Asset; digest = "sha256:$(Get-Sha (Join-Path $Dir "files\$Asset"))"
        browser_download_url = "https://dl.test/$Asset"
    })
    if ($Sums) {
        foreach ($Name in "SHA256SUMS", "SHA256SUMS.sig", "SHA256SUMS.v2.sig") {
            if (Test-Path -LiteralPath (Join-Path $Dir "files\$Name")) {
                $Assets += [pscustomobject]@{ name = $Name; browser_download_url = "https://dl.test/$Name" }
            }
        }
    }
    $script:Releases[$Dir] = [pscustomobject]@{ tag_name = $Tag; name = $Title; assets = $Assets }
}
$script:Releases = @{}

# install.ps1 with the fake environment: the user's PATH is never written.
function Set-Installer([string]$Dir) {
    $Text = [IO.File]::ReadAllText($Installer)
    $Patched = $Text -replace '\[Environment\]::', '[FakeEnvironment]::'
    if ($Patched.Contains("[Environment]::")) { throw "could not patch install.ps1" }
    Write-Ascii (Join-Path $Dir "install.ps1") $Patched
}

# Runs the case's install.ps1; returns @{ Ok; Out }. The verifier is the test build, which trusts the case's throwaway
# key (SUNRISE_VERIFY_TEST_KEY, or Key); Pin replaces its pin.
function Invoke-Case([string]$Dir, [hashtable]$Arguments = @{}, [string]$Key = "", [string]$Pin = $VerifySha) {
    $global:SunriseTestRel = [pscustomobject]@{ Release = $script:Releases[$Dir]; Files = Join-Path $Dir "files" }
    $env:SUNRISE_VERIFY_TEST_KEY = if ($Key) { $Key } else { $script:Keys[$Dir] }
    $env:SUNRISE_INSTALL_TEST_VERIFIER_SHA256 = $Pin
    $Ok = $true
    $Out = try {
        $Arguments.Dir = Join-Path $Dir "inst"
        & (Join-Path $Dir "install.ps1") @Arguments *>&1 | Out-String
    } catch { $Ok = $false; $_.Exception.Message }
    @{ Ok = $Ok; Out = "$Out" }
}

function Test-Case([string]$Name, [bool]$WantOk, [string]$Pattern, $Result) {
    if ($Result.Ok -eq $WantOk -and (-not $Pattern -or $Result.Out.Contains($Pattern))) {
        Write-Output "ok   $Name"
    } else {
        Write-Output "FAIL $Name (installed: $($Result.Ok), wanted $WantOk, pattern '$Pattern')"
        Write-Output (($Result.Out -split "`n" | ForEach-Object { "     $_" }) -join "`n")
        $script:Fails++
    }
}
function Test-That([string]$Name, [bool]$Condition) {
    if (-not $Condition) { Write-Output "FAIL $Name"; $script:Fails++ }
}

try {
    $Nightly = "nightly-2026-11-03-0c02067"
    $v = @{ Version = "v1.2.3" }

    # Passing cases.
    # 1. Good signature, a Nightly: SHA256SUMS.v2.sig signed for the version its title names.
    $d = Join-Path $W "good"; New-Release $d "nightly" $Nightly
    $r = Invoke-Case $d; Test-Case "good release installs (Nightly, signed for its own version)" $true "Release signature OK" $r
    Test-That "nothing installed" (Test-Path (Join-Path $d "inst\current\sunrise-server.exe"))
    Test-That "not installed as $Nightly" ($r.Out.Contains("Downloading Sunrise $Nightly "))

    # 2. Good signature, a normal tag.
    $d = Join-Path $W "tag"; New-Release $d "v1.2.3"
    $r = Invoke-Case $d $v; Test-Case "good release installs (release tag)" $true "Release signature OK" $r

    # 3. -InsecureChecksumOnly: installs on GitHub's SHA-256 with a loud warning, and needs no verifier at all.
    $d = Join-Path $W "insecure"; New-Release $d "v1.2.3"
    Remove-Item (Join-Path $d "files\$VerifierAsset"), (Join-Path $d "files\SHA256SUMS.sig"); Set-Release $d "v1.2.3" $false
    $r = Invoke-Case $d ($v + @{ InsecureChecksumOnly = $true }) "" "unpinned"; Test-Case "-InsecureChecksumOnly installs, with its warning" $true "-InsecureChecksumOnly" $r
    Test-That "insecure: nothing installed" (Test-Path (Join-Path $d "inst\current\sunrise-server.exe"))
    $d = Join-Path $W "insecure2"; New-Release $d "v1.2.3"; $script:Releases[$d].assets[0].digest = "sha256:0000"
    $r = Invoke-Case $d ($v + @{ InsecureChecksumOnly = $true }); Test-Case "-InsecureChecksumOnly still refuses a wrong SHA-256" $false "doesn't match its SHA-256" $r

    # The five tamper cases (every refusal names its cause).
    # T1. One byte of the archive flipped; GitHub's digest agrees with it, only the signed SHA256SUMS catches it.
    $d = Join-Path $W "arc"; New-Release $d "v1.2.3"
    Add-Content -LiteralPath (Join-Path $d "files\$Asset") -Value "x"; Set-Release $d "v1.2.3" $true
    $r = Invoke-Case $d $v; Test-Case "T1 flipped archive byte refused" $false "doesn't match its signed checksum" $r
    Test-That "flipped archive was installed" (-not (Test-Path (Join-Path $d "inst\current")))

    # T2. SHA256SUMS signed with another key.
    $d = Join-Path $W "wrong"; New-Release $d "v1.2.3"; $d2 = Join-Path $W "otherkey"; New-Release $d2 "v1.2.3"
    $r = Invoke-Case $d $v $script:Keys[$d2]; Test-Case "T2 SHA256SUMS signed with another key refused" $false "signature doesn't match Sunrise's release key" $r
    Test-That "another key was installed" (-not (Test-Path (Join-Path $d "inst\current")))
    # T2b. Its list replaced with the tampered archive's hash, the signature kept.
    $d = Join-Path $W "sums"; New-Release $d "v1.2.3"
    Add-Content -LiteralPath (Join-Path $d "files\$Asset") -Value "x"
    Write-Ascii (Join-Path $d "files\SHA256SUMS") "$(Get-Sha (Join-Path $d "files\$Asset"))  $Asset`n"; Set-Release $d "v1.2.3" $true
    $r = Invoke-Case $d $v; Test-Case "T2b tampered SHA256SUMS refused" $false "signature doesn't match" $r

    # T3. The .sig missing from the release.
    $d = Join-Path $W "nosig"; New-Release $d "v1.2.3"; Remove-Item (Join-Path $d "files\SHA256SUMS.sig"); Set-Release $d "v1.2.3" $true
    $r = Invoke-Case $d $v; Test-Case "T3 missing SHA256SUMS.sig refused" $false "has no signed checksums (SHA256SUMS.sig)" $r
    Test-That "missing .sig was installed" (-not (Test-Path (Join-Path $d "inst\current")))

    # T4. The verifier swapped for another binary: its hash isn't the pinned one, so it never runs.
    $d = Join-Path $W "swap"; New-Release $d "v1.2.3"
    Write-Ascii (Join-Path $d "files\$VerifierAsset") "not the verifier"
    $r = Invoke-Case $d $v; Test-Case "T4 swapped verifier refused (hash mismatch)" $false "isn't the pinned build" $r
    Test-That "swapped verifier was installed" (-not (Test-Path (Join-Path $d "inst\current")))

    # T5. No verifier for this platform (no pin), and a verifier whose download fails.
    $d = Join-Path $W "nopin"; New-Release $d "v1.2.3"
    $r = Invoke-Case $d $v "" "unpinned"; Test-Case "T5 no verifier for the platform refused" $false "There is no sunrise-verify for windows-x64" $r
    $d = Join-Path $W "nodl"; New-Release $d "v1.2.3"; Remove-Item (Join-Path $d "files\$VerifierAsset")
    $r = Invoke-Case $d $v; Test-Case "T5 verifier download failure refused" $false "Could not download sunrise-verify" $r
    Test-That "no verifier download was installed" (-not (Test-Path (Join-Path $d "inst\current")))

    # The placeholder pin refuses, and the test overrides apply only to a -Dir inside SUNRISE_INSTALL_TEST_ROOT.
    $d = Join-Path $W "unpinned"; New-Release $d "v1.2.3"
    $Text = [IO.File]::ReadAllText((Join-Path $d "install.ps1")) -replace "(?m)^\$VerifierVersion = .*$", "`$VerifierVersion = 'unpinned'"
    Write-Ascii (Join-Path $d "install.ps1") $Text
    $env:SUNRISE_INSTALL_TEST_VERIFIER_VERSION = $null
    $r = Invoke-Case $d $v; Test-Case "unpinned script refused" $false "has no pinned sunrise-verify" $r
    $env:SUNRISE_INSTALL_TEST_VERIFIER_VERSION = "test"
    $d = Join-Path $W "outside"; New-Release $d "v1.2.3"
    $env:SUNRISE_INSTALL_TEST_ROOT = Join-Path $W "elsewhere"
    $r = Invoke-Case $d $v; Test-Case "a pin override outside the test root is ignored" $false "" $r
    Test-That "override outside the root was honoured" (-not $r.Out.Contains("Release signature OK"))
    $env:SUNRISE_INSTALL_TEST_ROOT = $W

    # Nightly and replay cases from before.
    # N1. A Nightly with only the channel's signature (published before v2): refused, never the channel's signature.
    $d = Join-Path $W "channel"; New-Release $d "nightly"; Set-Release $d "nightly" $true (Get-Title "nightly" $Nightly)
    $r = Invoke-Case $d; Test-Case "Nightly without SHA256SUMS.v2.sig refused" $false "has no SHA256SUMS.v2.sig" $r
    Test-That "a Nightly without v2 was installed" (-not (Test-Path (Join-Path $d "inst\current")))

    # N2. An older Nightly's files and signature retitled as a newer one: its v2 signature names the old version.
    $d = Join-Path $W "rollback"; New-Release $d "nightly" "nightly-2026-11-01-1111111"
    Set-Release $d "nightly" $true (Get-Title "nightly" $Nightly)
    $r = Invoke-Case $d; Test-Case "older Nightly passed off as a newer one refused" $false "signature doesn't match" $r

    # N3. A Nightly whose title doesn't name its date and commit: refused, not dated some other way.
    $d = Join-Path $W "title"; New-Release $d "nightly" $Nightly; Set-Release $d "nightly" $true "Nightly"
    $r = Invoke-Case $d; Test-Case "Nightly with an unreadable title refused" $false "title doesn't name its date and commit" $r

    # N4. A signature made for another tag does not pass (a release's list replayed under another tag).
    $d = Join-Path $W "replay"; New-Release $d "v1.2.3"; Set-Release $d "v9.9.9" $true
    $r = Invoke-Case $d @{ Version = "v9.9.9" }; Test-Case "signature replayed under another tag refused" $false "signature doesn't match" $r

    # N5. A release published before signing (no SHA256SUMS): refused now; the escape hatch is the flag above.
    $d = Join-Path $W "legacy"; New-Release $d "v1.2.3"; Set-Release $d "v1.2.3" $false
    $r = Invoke-Case $d $v; Test-Case "release without signed checksums refused" $false "has no signed checksums" $r
} finally {
    Remove-Item -Recurse -Force $W -ErrorAction SilentlyContinue
}

Write-Output "$($script:Fails) case(s) failed"
if ($script:Fails -ne 0) { exit 1 }
exit 0
