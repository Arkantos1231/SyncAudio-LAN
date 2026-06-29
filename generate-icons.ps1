# generate-icons.ps1
# Crea iconos placeholder para compilar el proyecto.
# Ejecuta este script UNA VEZ antes de `npm install && cargo tauri dev`.
# Para iconos de produccion, reemplaza los archivos en src-tauri\icons\ con tus propios.

param()
Add-Type -AssemblyName System.Drawing

$iconsDir = Join-Path $PSScriptRoot "src-tauri\icons"
if (-not (Test-Path $iconsDir)) {
    New-Item -ItemType Directory -Path $iconsDir | Out-Null
}

function New-PlaceholderPng {
    param([int]$Size, [string]$Path)
    $bmp = New-Object System.Drawing.Bitmap($Size, $Size)
    $g   = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    # Fondo oscuro
    $bg = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 15, 17, 23))
    $g.FillRectangle($bg, 0, 0, $Size, $Size)
    # Circulo azul
    $margin = [int]($Size * 0.1)
    $brush  = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 59, 130, 246))
    $g.FillEllipse($brush, $margin, $margin, $Size - 2*$margin, $Size - 2*$margin)
    $g.Dispose(); $bg.Dispose(); $brush.Dispose()
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Host "Creado: $Path"
}

function New-PlaceholderIco {
    param([string]$SourcePng, [string]$IcoPath)
    # Genera un .ico minimal a partir del PNG usando System.Drawing
    $src = [System.Drawing.Image]::FromFile($SourcePng)
    $ico32 = New-Object System.Drawing.Bitmap(32, 32)
    $g = [System.Drawing.Graphics]::FromImage($ico32)
    $g.DrawImage($src, 0, 0, 32, 32)
    $g.Dispose(); $src.Dispose()

    $ms = New-Object System.IO.MemoryStream
    $ico32.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $ico32.Dispose()

    # Escribir estructura ICO manual con el PNG embebido
    $pngBytes = $ms.ToArray()
    $ms.Dispose()
    $bw = New-Object System.IO.BinaryWriter([System.IO.File]::Create($IcoPath))
    # ICO header
    $bw.Write([uint16]0)   # reserved
    $bw.Write([uint16]1)   # type=ICO
    $bw.Write([uint16]1)   # count=1
    # ICONDIRENTRY
    $bw.Write([byte]32)    # width
    $bw.Write([byte]32)    # height
    $bw.Write([byte]0)     # color count
    $bw.Write([byte]0)     # reserved
    $bw.Write([uint16]1)   # planes
    $bw.Write([uint16]32)  # bit count
    $bw.Write([uint32]$pngBytes.Length)   # size
    $bw.Write([uint32]22)  # offset (6 header + 16 entry)
    # PNG data
    $bw.Write($pngBytes)
    $bw.Close()
    Write-Host "Creado: $IcoPath"
}

# Generar PNGs en los tamanos requeridos por Tauri
New-PlaceholderPng -Size 32  -Path (Join-Path $iconsDir "32x32.png")
New-PlaceholderPng -Size 128 -Path (Join-Path $iconsDir "128x128.png")
New-PlaceholderPng -Size 256 -Path (Join-Path $iconsDir "128x128@2x.png")

# Generar ICO desde el PNG de 32x32
$png32 = Join-Path $iconsDir "32x32.png"
New-PlaceholderIco -SourcePng $png32 -IcoPath (Join-Path $iconsDir "icon.ico")

Write-Host ""
Write-Host "Iconos placeholder generados en: $iconsDir"
Write-Host "Ahora ejecuta: npm install && npm run tauri dev"
