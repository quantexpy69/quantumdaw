# Publica una versión nueva de Quantum DAW: sube la versión de los crates, hace el commit con las
# notas, crea la etiqueta vX.Y.Z y lo sube a GitHub. Al llegar la etiqueta, GitHub Actions compila
# Windows, macOS y Linux y publica el release; las apps instaladas lo ofrecen como actualización.
#
# Uso (desde la carpeta del proyecto):
#   .\packaging\publicar.ps1 0.62.0 notas.txt
# notas.txt: la primera línea es el título del commit; después, secciones y viñetas «- …», por ejemplo:
#   Quantum DAW 0.62.0
#
#   Edición
#   - Arreglado …
param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$Notas
)
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "La versión debe ser X.Y.Z (por ejemplo 0.62.0)" }
if (-not (Test-Path $Notas)) { throw "No existe el archivo de notas: $Notas" }
$raiz = Split-Path -Parent $PSScriptRoot
Set-Location $raiz
$env:Path += ";C:\Program Files\Git\cmd;$env:USERPROFILE\.cargo\bin"

$actual = (Select-String -Path crates/app/Cargo.toml -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
if ($actual -eq $Version) { throw "El proyecto ya está en la versión $Version" }
if (git tag --list "v$Version") { throw "La etiqueta v$Version ya existe" }

foreach ($f in 'crates/app/Cargo.toml', 'crates/engine/Cargo.toml', 'crates/project/Cargo.toml') {
    $t = [IO.File]::ReadAllText("$raiz/$f").Replace("version = `"$actual`"", "version = `"$Version`"")
    [IO.File]::WriteAllText("$raiz/$f", $t)
}
cargo update -w --quiet
cargo build -p quantum-daw --quiet
if ($LASTEXITCODE -ne 0) { throw "No compila: revisa los errores antes de publicar" }

# git escribe avisos por stderr (por ejemplo los de fin de línea) y PowerShell 5 los tomaría como
# errores: se ejecuta sin cortar y se mira el código de salida.
function Git { $ErrorActionPreference = 'Continue'; & git @args 2>&1 | Where-Object { "$_" -notmatch 'LF will be replaced' } | ForEach-Object { "$_" }; if ($LASTEXITCODE -ne 0) { throw "git $args falló" } }
Git add -A
Git commit -q -F $Notas
Git tag "v$Version"
Git push -q origin main
Git push -q origin "v$Version"
Write-Host "Publicada v$Version. Sigue la compilación en https://github.com/quantexpy69/quantumdaw/actions"
