# Generates the mod's 16x16 pixel-art textures. Every pattern is drawn from code with a fixed
# seed, so running it again gives the same PNG files byte for byte.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/generate_textures.ps1
#
# Output: assets/create/textures/block/*.png and assets/create/textures/item/*.png
Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
$blockDir = Join-Path $root 'assets/create/textures/block'
$itemDir = Join-Path $root 'assets/create/textures/item'
New-Item -ItemType Directory -Force $blockDir, $itemDir | Out-Null

function Clamp([double]$v) { [int][math]::Max(0, [math]::Min(255, [math]::Round($v))) }

# A texture under construction: a 16x16 grid of ARGB colours, written out at the end.
function New-Canvas { , (New-Object 'object[]' 256) }
function Put($canvas, [int]$x, [int]$y, $r, $g, $b, [int]$a = 255) {
    if ($x -lt 0 -or $x -gt 15 -or $y -lt 0 -or $y -gt 15) { return }
    $canvas[$y * 16 + $x] = [System.Drawing.Color]::FromArgb($a, (Clamp $r), (Clamp $g), (Clamp $b))
}
function Fill($canvas, $r, $g, $b) { for ($i = 0; $i -lt 256; $i++) { Put $canvas ($i % 16) ([math]::Floor($i / 16)) $r $g $b } }
function Shade($canvas, [int]$x, [int]$y, [double]$d) {
    $c = $canvas[$y * 16 + $x]
    if ($null -ne $c) { Put $canvas $x $y ($c.R + $d) ($c.G + $d) ($c.B + $d) $c.A }
}
function Noise($canvas, $rng, [double]$amount) {
    for ($y = 0; $y -lt 16; $y++) { for ($x = 0; $x -lt 16; $x++) {
        Shade $canvas $x $y (($rng.NextDouble() - 0.5) * 2 * $amount)
    } }
}
function Border($canvas, [double]$dark, [double]$light) {
    for ($i = 0; $i -lt 16; $i++) {
        Shade $canvas $i 15 $dark; Shade $canvas 15 $i $dark   # bottom and right: shadow
        Shade $canvas $i 0 $light; Shade $canvas 0 $i $light   # top and left: highlight
    }
}
function Save-Canvas($canvas, $dir, $name) {
    $bmp = New-Object System.Drawing.Bitmap 16, 16, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    for ($i = 0; $i -lt 256; $i++) {
        $c = $canvas[$i]; if ($null -eq $c) { $c = [System.Drawing.Color]::Transparent }
        $bmp.SetPixel($i % 16, [math]::Floor($i / 16), $c)
    }
    $bmp.Save((Join-Path $dir "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    "  $name.png"
}
function Rng([string]$name) { New-Object System.Random ([math]::Abs($name.GetHashCode() % 100000) + 17) }
# String.GetHashCode is randomised per process on some runtimes, so derive the seed by hand.
function Seed([string]$name) { $h = 7; foreach ($ch in $name.ToCharArray()) { $h = ($h * 31 + [int]$ch) % 100003 }; New-Object System.Random $h }
function Rivet($canvas, $x, $y) {
    Put $canvas $x $y 236 188 140
    Put $canvas ($x + 1) $y 210 160 112
    Put $canvas $x ($y + 1) 150 92 54
    Put $canvas ($x + 1) ($y + 1) 120 70 40
}

"Blocks:"
# Brushed steel: shafts, axles and stubs. Vertical streaks, a bright column and a dark one.
$c = New-Canvas; $r = Seed 'steel'
for ($y = 0; $y -lt 16; $y++) { for ($x = 0; $x -lt 16; $x++) {
    $base = 118 + @(0, 7, -3, 4)[$x % 4] + ($r.NextDouble() - 0.5) * 6
    Put $c $x $y ($base - 5) $base ($base + 7)
} }
foreach ($y in 0..15) { Shade $c 2 $y 14; Shade $c 3 $y 8; Shade $c 12 $y -14; Shade $c 13 $y -8 }
Save-Canvas $c $blockDir 'steel'

# Wood: the discs of the cogwheels. Planks with a grain.
$c = New-Canvas; $r = Seed 'wood_cog'
for ($y = 0; $y -lt 16; $y++) { for ($x = 0; $x -lt 16; $x++) {
    $grain = @(0, -10, -4, 6)[$y % 4]
    Put $c $x $y (152 + $grain) (106 + $grain) (58 + $grain)
} }
Noise $c $r 7
foreach ($x in 0..15) { Shade $c $x 0 -26; Shade $c $x 7 -20; Shade $c $x 8 10; Shade $c $x 15 -26 }
foreach ($y in 1..6) { Shade $c 0 $y -12; Shade $c 15 $y -12 }
Save-Canvas $c $blockDir 'wood_cog'

# Copper plates with rivets: motor body, gearbox casing, stressometer body.
$c = New-Canvas; $r = Seed 'copper_plate'
Fill $c 178 108 62
Noise $c $r 6
Border $c -38 22
foreach ($x in 0..15) { Shade $c $x 7 -10; Shade $c $x 8 12 }
Rivet $c 2 2; Rivet $c 12 2; Rivet $c 2 12; Rivet $c 12 12
Save-Canvas $c $blockDir 'copper_plate'

# Dark casing: the drill's body. Bluish plates with bolts.
$c = New-Canvas; $r = Seed 'dark_casing'
Fill $c 58 64 78
Noise $c $r 4
Border $c -22 14
foreach ($p in @(@(3, 3), @(12, 3), @(3, 12), @(12, 12))) { Put $c $p[0] $p[1] 128 138 152; Put $c ($p[0] + 1) $p[1] 96 104 118; Put $c $p[0] ($p[1] + 1) 86 94 106 }
foreach ($x in 4..11) { Shade $c $x 7 -12; Shade $c $x 8 8 }
Save-Canvas $c $blockDir 'dark_casing'

# Drill metal: the head's base. Lighter steel in horizontal bands.
$c = New-Canvas; $r = Seed 'drill_metal'
for ($y = 0; $y -lt 16; $y++) { for ($x = 0; $x -lt 16; $x++) {
    $band = if (($y % 6) -lt 2) { -26 } else { 0 }
    Put $c $x $y (172 + $band) (176 + $band) (184 + $band)
} }
Noise $c $r 5
Border $c -18 16
Save-Canvas $c $blockDir 'drill_metal'

# Drill bit: orange steel with diagonal spiral stripes.
$c = New-Canvas; $r = Seed 'drill_bit'
for ($y = 0; $y -lt 16; $y++) { for ($x = 0; $x -lt 16; $x++) {
    if ((($x + $y) % 4) -lt 2) { Put $c $x $y 236 168 78 } else { Put $c $x $y 192 112 40 }
} }
Noise $c $r 5
Save-Canvas $c $blockDir 'drill_bit'

# Gauge body: light steel panel with a dark frame.
$c = New-Canvas; $r = Seed 'gauge_body'
Fill $c 152 158 166
Noise $c $r 4
Border $c -34 20
foreach ($i in 1..14) { Shade $c $i 1 -10; Shade $c 1 $i -10; Shade $c $i 14 12; Shade $c 14 $i 12 }
Save-Canvas $c $blockDir 'gauge_body'

# Dials: a cream face with a ring and tick marks every 30 degrees; the stressometer's ring runs
# from green to red along the needle's sweep.
function Draw-Dial([bool]$stress, [string]$name) {
    $c = New-Canvas; $r = Seed $name
    for ($y = 0; $y -lt 16; $y++) { for ($x = 0; $x -lt 16; $x++) {
        $dx = $x - 7.5; $dy = $y - 7.5; $d = [math]::Sqrt($dx * $dx + $dy * $dy)
        if ($d -gt 7.6) { Put $c $x $y 70 76 88; continue }
        Put $c $x $y 234 228 206
        if ($d -gt 6.4) { Put $c $x $y 52 56 66; continue }
        if ($stress -and $d -gt 4.4 -and $d -le 6.4) {
            # Angle from straight up, clockwise; the arc covers -135 to +135 degrees.
            $a = [math]::Atan2($dx, -$dy) * 180 / [math]::PI
            if ($a -ge -135 -and $a -le 135) {
                $t = ($a + 135) / 270
                Put $c $x $y (60 + 150 * $t) (176 - 110 * $t) (76 - 26 * $t)
            }
        }
    } }
    foreach ($k in 0..8) {
        $a = (-135 + 33.75 * $k) * [math]::PI / 180
        $tx = [math]::Round(7.5 + 5.3 * [math]::Sin($a)); $ty = [math]::Round(7.5 - 5.3 * [math]::Cos($a))
        if (-not $stress) { Put $c $tx $ty 40 84 170 }
    }
    Put $c 7 7 52 56 66; Put $c 8 7 52 56 66; Put $c 7 8 52 56 66; Put $c 8 8 52 56 66
    Noise $c $r 2
    Save-Canvas $c $blockDir $name
}
Draw-Dial $false 'speedometer_dial'
Draw-Dial $true 'stressometer_dial'

# Needle: a flat red.
$c = New-Canvas; $r = Seed 'needle'
Fill $c 204 34 34
Noise $c $r 5
Save-Canvas $c $blockDir 'needle'

# Clutch and gearshift cores, plain and powered.
function Draw-Core([string]$name, $base, $stripe) {
    $c = New-Canvas; $r = Seed $name
    for ($y = 0; $y -lt 16; $y++) { for ($x = 0; $x -lt 16; $x++) {
        $s = if (($y % 4) -lt 1) { 1 } else { 0 }
        Put $c $x $y ($base[0] + $s * $stripe) ($base[1] + $s * $stripe) ($base[2] + $s * $stripe)
    } }
    Noise $c $r 5
    Border $c -26 16
    Save-Canvas $c $blockDir $name
}
Draw-Core 'clutch_core' @(176, 108, 62) 22
Draw-Core 'clutch_core_powered' @(206, 44, 40) 30
Draw-Core 'gearshift_core' @(228, 190, 70) 18
Draw-Core 'gearshift_core_powered' @(222, 70, 40) 28

# Saw blade: bright steel with a row of dark teeth along the edges.
$c = New-Canvas; $r = Seed 'saw_blade'
for ($y = 0; $y -lt 16; $y++) { for ($x = 0; $x -lt 16; $x++) {
    $b = 196 + @(0, 6, -4, 3)[$x % 4]
    Put $c $x $y ($b - 6) $b ($b + 6)
} }
Noise $c $r 4
foreach ($i in 0..15) { if (($i % 3) -eq 0) { Shade $c $i 0 -70; Shade $c $i 15 -70; Shade $c 0 $i -70; Shade $c 15 $i -70 } else { Shade $c $i 0 -24; Shade $c $i 15 -24 } }
foreach ($y in 6..9) { Shade $c 7 $y -30; Shade $c 8 $y -30 }
Save-Canvas $c $blockDir 'saw_blade'

# Harvester blade: green-tinted steel with a bright cutting edge on one side.
$c = New-Canvas; $r = Seed 'harvester_blade'
for ($y = 0; $y -lt 16; $y++) { for ($x = 0; $x -lt 16; $x++) {
    $band = @(0, 8, -6, 4)[$y % 4]
    Put $c $x $y (96 + $band) (152 + $band) (98 + $band)
} }
Noise $c $r 5
foreach ($x in 0..15) { Shade $c $x 0 40; Shade $c $x 1 18; Shade $c $x 15 -34 }
Save-Canvas $c $blockDir 'harvester_blade'

# Plough share: dark iron with a rusty brown wash and vertical grooves.
$c = New-Canvas; $r = Seed 'plough_share'
for ($y = 0; $y -lt 16; $y++) { for ($x = 0; $x -lt 16; $x++) {
    $g = if (($x % 4) -eq 0) { -22 } else { 0 }
    Put $c $x $y (122 + $g) (98 + $g) (76 + $g)
} }
Noise $c $r 7
Border $c -30 18
Save-Canvas $c $blockDir 'plough_share'

# Belt surface: dark rubber with a lighter bar across every four pixels. It repeats every four rows,
# so it scrolls without a seam when the mod moves it along the belt.
$c = New-Canvas; $r = Seed 'belt_surface'
for ($y = 0; $y -lt 16; $y++) { for ($x = 0; $x -lt 16; $x++) {
    if (($y % 4) -eq 0) { Put $c $x $y 92 92 100 }
    elseif (($y % 4) -eq 1) { Put $c $x $y 62 62 70 }
    else { Put $c $x $y 44 44 50 }
} }
foreach ($y in 0..15) { Shade $c 0 $y -14; Shade $c 15 $y -14 }
Save-Canvas $c $blockDir 'belt_surface'

"Items:"
# The wrench: a steel handle running down-left and an open jaw at the top right.
$c = New-Canvas
$steel = @(150, 158, 170); $dark = @(60, 66, 78); $light = @(206, 212, 222)
for ($i = 0; $i -le 8; $i++) {
    $x = 3 + $i; $y = 12 - $i
    foreach ($o in @(@(0, 0), @(1, 0), @(0, 1))) { Put $c ($x + $o[0]) ($y + $o[1]) $steel[0] $steel[1] $steel[2] }
    Put $c ($x - 1) $y $dark[0] $dark[1] $dark[2]
    Put $c ($x + 1) ($y + 1) $dark[0] $dark[1] $dark[2]
}
for ($y = 1; $y -le 8; $y++) { for ($x = 7; $x -le 14; $x++) {
    $dx = $x - 11; $dy = $y - 4.5; $d = [math]::Sqrt($dx * $dx + $dy * $dy)
    if ($d -le 3.3 -and $d -ge 1.2) { Put $c $x $y $steel[0] $steel[1] $steel[2] }
    elseif ($d -gt 3.3 -and $d -le 4.1) { Put $c $x $y $dark[0] $dark[1] $dark[2] }
} }
# The jaw opens toward the upper right.
foreach ($p in @(@(12, 2), @(13, 2), @(13, 3), @(14, 3), @(12, 3), @(14, 2), @(13, 1), @(12, 1))) { Put $c $p[0] $p[1] 0 0 0 0 }
Put $c 6 8 $light[0] $light[1] $light[2]; Put $c 5 9 $light[0] $light[1] $light[2]; Put $c 9 5 $light[0] $light[1] $light[2]
Save-Canvas $c $itemDir 'wrench'
"done"
