Add-Type -AssemblyName System.Drawing

$size = 256
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = 'AntiAlias'

# 蓝色背景
$g.Clear([System.Drawing.Color]::FromArgb(0, 120, 212))

# 白色播放三角形
$brush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)
$padding = [int]($size * 0.2)
$playSize = [int]($size - $padding * 2)

# 三角形三个点
$p1 = New-Object System.Drawing.Point(($padding + [int]($playSize * 0.25)), $padding)
$p2 = New-Object System.Drawing.Point(($padding + [int]($playSize * 0.25)), ($padding + $playSize))
$p3 = New-Object System.Drawing.Point(($padding + $playSize), ($padding + [int]($playSize * 0.5)))

$points = @($p1, $p2, $p3)
$g.FillPolygon($brush, $points)

# 扫描线效果
$pen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(60, 255, 255, 255), 2)
for ($i = 0; $i -lt 5; $i++) {
    $y = [int]($size * 0.3) + $i * [int]($size * 0.12)
    $g.DrawLine($pen, [int]($size * 0.15), $y, [int]($size * 0.85), $y)
}

$g.Dispose()
$bmp.Save("d:\_codes\_my_ai\my-video-tauri\src-tauri\icons\256x256.png", [System.Drawing.Imaging.ImageFormat]::Png)

# 缩放成各种尺寸
$sizes = @{
    "32x32.png" = 32
    "128x128.png" = 128
    "128x128@2x.png" = 256
    "icon.png" = 256
}

foreach ($name in $sizes.Keys) {
    $newSize = $sizes[$name]
    $resized = New-Object System.Drawing.Bitmap($newSize, $newSize)
    $gr = [System.Drawing.Graphics]::FromImage($resized)
    $gr.InterpolationMode = 'HighQualityBicubic'
    $gr.DrawImage($bmp, 0, 0, $newSize, $newSize)
    $gr.Dispose()
    $resized.Save("d:\_codes\_my_ai\my-video-tauri\src-tauri\icons\$name", [System.Drawing.Imaging.ImageFormat]::Png)
    $resized.Dispose()
}

$bmp.Dispose()

# 生成 ICO
$ico256 = [System.Drawing.Image]::FromFile("d:\_codes\_my_ai\my-video-tauri\src-tauri\icons\256x256.png")
$ico = [System.Drawing.Icon]::FromHandle(([System.Drawing.Bitmap]$ico256).GetHicon())
$fs = [System.IO.File]::Create("d:\_codes\_my_ai\my-video-tauri\src-tauri\icons\icon.ico")
$ico.Save($fs)
$fs.Close()
$ico256.Dispose()

Write-Host "图标已生成"
