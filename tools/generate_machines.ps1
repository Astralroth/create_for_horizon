# Generates the files of the M4 and M5 blocks: Hand Crank, Water Wheel, Mechanical Saw,
# Mechanical Harvester and Mechanical Plough. Their block states have many combinations (facing
# x speed x reverse ...), so the block definitions, block states, models, item definitions, loot
# tables and recipes are written from code and the output is committed.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/generate_machines.ps1
#
# Everything here is deterministic: running it again gives the same files byte for byte.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$inv = [System.Globalization.CultureInfo]::InvariantCulture
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Write-File([string]$relative, [string]$text) {
    $path = Join-Path $root $relative
    New-Item -ItemType Directory -Force (Split-Path -Parent $path) | Out-Null
    [System.IO.File]::WriteAllText($path, $text.Replace("`r`n", "`n"), $utf8)
    "  $relative"
}
function Num([double]$v) { $v.ToString('0.####', $inv) }
function Nums($values) { '[' + (($values | ForEach-Object { Num $_ }) -join ', ') + ']' }

$speeds = @('0', '1', '2', '4', '8', '16', '32', '64', '128', '256')
$facings = @('north', 'south', 'east', 'west', 'up', 'down')
$speedValues = '["' + ($speeds -join '", "') + '"]'

# --- Models ---------------------------------------------------------------------------------
# One cuboid with box-projected UVs, the way the rest of the mod's models are written.
function Elem($from, $to, [string]$tex) {
    $f = $from; $t = $to
    $lines = @(
        '    {',
        "      ""from"": $(Nums $f),",
        "      ""to"": $(Nums $t),",
        '      "faces": {',
        "        ""down"": { ""uv"": $(Nums @($f[0], $f[2], $t[0], $t[2])), ""texture"": ""#$tex"" },",
        "        ""up"": { ""uv"": $(Nums @($f[0], $f[2], $t[0], $t[2])), ""texture"": ""#$tex"" },",
        "        ""north"": { ""uv"": $(Nums @($f[0], $f[1], $t[0], $t[1])), ""texture"": ""#$tex"" },",
        "        ""south"": { ""uv"": $(Nums @($f[0], $f[1], $t[0], $t[1])), ""texture"": ""#$tex"" },",
        "        ""west"": { ""uv"": $(Nums @($f[2], $f[1], $t[2], $t[1])), ""texture"": ""#$tex"" },",
        "        ""east"": { ""uv"": $(Nums @($f[2], $f[1], $t[2], $t[1])), ""texture"": ""#$tex"" }",
        '      }',
        '    }'
    )
    $lines -join "`n"
}
function Model($textures, $elements) {
    $tex = ($textures.GetEnumerator() | ForEach-Object { "    ""$($_.Key)"": ""$($_.Value)""" }) -join ",`n"
    "{`n  ""textures"": {`n$tex`n  },`n  ""elements"": [`n" + ($elements -join ",`n") + "`n  ]`n}`n"
}

# --- Shapes ---------------------------------------------------------------------------------
# Boxes are written for a block facing north (its back toward +z) and turned to each facing.
function Turn-Point($p, [string]$facing) {
    $x = $p[0]; $y = $p[1]; $z = $p[2]
    switch ($facing) {
        'north' { return @($x, $y, $z) }
        'south' { return @((1 - $x), $y, (1 - $z)) }
        'east'  { return @((1 - $z), $y, $x) }
        'west'  { return @($z, $y, (1 - $x)) }
        'up'    { return @($x, (1 - $z), $y) }
        'down'  { return @($x, $z, $y) }
    }
}
function Turn-Box($b, [string]$facing) {
    $a = Turn-Point @($b[0], $b[1], $b[2]) $facing
    $c = Turn-Point @($b[3], $b[4], $b[5]) $facing
    @([math]::Min($a[0], $c[0]), [math]::Min($a[1], $c[1]), [math]::Min($a[2], $c[2]),
      [math]::Max($a[0], $c[0]), [math]::Max($a[1], $c[1]), [math]::Max($a[2], $c[2]))
}
function Swap-Box($b, [string]$axis) {
    # A box written for axis y, laid along another axis.
    switch ($axis) {
        'y' { return $b }
        'x' { return @($b[1], $b[0], $b[2], $b[4], $b[3], $b[5]) }
        'z' { return @($b[0], $b[2], $b[1], $b[3], $b[5], $b[4]) }
    }
}
function Shapes-Json([string]$property, $names, [scriptblock]$boxesFor) {
    $entries = foreach ($name in $names) {
        $boxes = @(& $boxesFor $name)
        "      ""$name"": [" + (($boxes | ForEach-Object { Nums $_ }) -join ', ') + ']'
    }
    "{`n    ""properties"": [""$property""],`n    ""values"": {`n" + ($entries -join ",`n") + "`n    }`n  }"
}

# --- Block definitions ----------------------------------------------------------------------
function Block-Json([string]$template, [string]$stateProperties, [string]$defaults, [double]$hardness, [double]$resistance, [bool]$requiresTool, [string]$sounds, [string]$shapes) {
    $tool = if ($requiresTool) { 'true' } else { 'false' }
    "{`n  ""template"": ""$template"",`n  ""state_properties"": {`n$stateProperties`n  },`n  ""default_state"": { $defaults },`n  ""properties"": {`n    ""hardness"": $(Num $hardness),`n    ""explosion_resistance"": $(Num $resistance),`n    ""requires_tool"": $tool,`n    ""occludes"": false,`n    ""render_shape"": ""ENTITYBLOCK_ANIMATED"",`n    ""sounds"": ""$sounds""`n  },`n  ""shapes"": $shapes,`n  ""collision_shapes"": $shapes`n}`n"
}
$facingProp = '    "facing": { "type": "enum", "values": ["north", "south", "east", "west", "up", "down"] }'
$speedProp = "    ""speed"": { ""type"": ""enum"", ""values"": $speedValues }"
$reverseProp = '    "reverse": { "type": "boolean" }'

# --- Block states ---------------------------------------------------------------------------
$facingTurn = @{
    north = ''; south = ', "y": 180'; east = ', "y": 90'; west = ', "y": 270'; up = ', "x": 270'; down = ', "x": 90'
}
function Variants($lines) { "{`n  ""variants"": {`n" + ($lines -join ",`n") + "`n  }`n}`n" }

# --- Items, loot, recipes -------------------------------------------------------------------
function Item-Json([string]$model) { "{`n  ""model"": { ""type"": ""minecraft:model"", ""model"": ""create:block/$model"" }`n}`n" }
function Loot-Json([string]$name) {
    "{`n  ""type"": ""minecraft:block"",`n  ""pools"": [`n    {`n      ""condition"": { ""type"": ""minecraft:survives_explosion"" },`n      ""entries"": [{ ""type"": ""minecraft:item"", ""name"": ""create:$name"" }],`n      ""rolls"": 1`n    }`n  ],`n  ""random_sequence"": ""create:blocks/$name""`n}`n"
}
function Recipe-Json([string]$name, $ingredients) {
    $list = ($ingredients | ForEach-Object { """$_""" }) -join ', '
    "{`n  ""type"": ""minecraft:crafting_shapeless"",`n  ""category"": ""misc"",`n  ""ingredients"": [$list],`n  ""result"": { ""id"": ""create:$name"" }`n}`n"
}

"Hand Crank:"
# Facing north: a plate on the back (the wall), a shaft out of it and an arm with a grip.
$crankElements = @(
    (Elem @(4, 4, 13) @(12, 12, 16) 'plate'),
    (Elem @(6, 6, 6) @(10, 10, 13) 'axle'),
    (Elem @(7, 7, 4) @(9, 14, 7) 'handle'),
    (Elem @(7, 12, 0) @(9, 14, 4) 'handle')
)
$crankTextures = [ordered]@{ plate = 'create:block/copper_plate'; axle = 'create:block/steel'; handle = 'create:block/wood_cog'; particle = 'create:block/copper_plate' }
Write-File 'assets/create/models/block/hand_crank.json' (Model $crankTextures $crankElements)
$crankBoxes = @(@(0.25, 0.25, 0.8125, 0.75, 0.75, 1), @(0.375, 0.375, 0.375, 0.625, 0.625, 0.8125))
$crankShapes = Shapes-Json 'facing' $facings { param($f) $crankBoxes | ForEach-Object { , (Turn-Box $_ $f) } }
$crankState = "$facingProp,`n    ""charge"": { ""type"": ""enum"", ""values"": [""0"", ""1"", ""2"", ""3"", ""4""] },`n$speedProp,`n$reverseProp"
Write-File 'data/create/horizon/block/hand_crank.json' (Block-Json 'minecraft:stone' $crankState '"facing": "north", "charge": "0", "speed": "0", "reverse": "false"' 1.5 3.0 $false 'minecraft:oak_planks' $crankShapes)
$lines = foreach ($c in 0..4) { foreach ($f in $facings) { foreach ($r in 'false', 'true') { foreach ($s in $speeds) {
    "    ""charge=$c,facing=$f,reverse=$r,speed=$s"": { ""model"": ""create:block/hand_crank""$($facingTurn[$f]) }"
} } } }
Write-File 'assets/create/blockstates/hand_crank.json' (Variants $lines)
Write-File 'assets/create/items/hand_crank.json' (Item-Json 'hand_crank')

"Water Wheel:"
# Axis y: a hub through the middle and two crossing bars with a blade on each end.
$wheelElements = @(
    (Elem @(6, 0, 6) @(10, 16, 10) 'hub'),
    (Elem @(0, 6, 6) @(16, 10, 10) 'wood'),
    (Elem @(6, 6, 0) @(10, 10, 16) 'wood'),
    (Elem @(0, 5, 4) @(2, 11, 12) 'wood'),
    (Elem @(14, 5, 4) @(16, 11, 12) 'wood'),
    (Elem @(4, 5, 0) @(12, 11, 2) 'wood'),
    (Elem @(4, 5, 14) @(12, 11, 16) 'wood')
)
$wheelTextures = [ordered]@{ hub = 'create:block/steel'; wood = 'create:block/wood_cog'; particle = 'create:block/wood_cog' }
Write-File 'assets/create/models/block/water_wheel.json' (Model $wheelTextures $wheelElements)
$wheelBoxes = @(@(0, 0.375, 0.375, 1, 0.625, 0.625), @(0.375, 0.375, 0, 0.625, 0.625, 1), @(0.375, 0, 0.375, 0.625, 1, 0.625))
$wheelShapes = Shapes-Json 'axis' @('x', 'y', 'z') { param($a) $wheelBoxes | ForEach-Object { , (Swap-Box $_ $a) } }
$wheelState = "$speedProp,`n$reverseProp,`n    ""invert"": { ""type"": ""boolean"" }"
Write-File 'data/create/horizon/block/water_wheel.json' (Block-Json 'minecraft:oak_log' $wheelState '"speed": "0", "reverse": "false", "invert": "false"' 2.0 3.0 $false 'minecraft:oak_planks' $wheelShapes)
$axisTurn = @{ y = ''; z = ', "x": 90'; x = ', "x": 90, "y": 90' }
$lines = foreach ($a in 'y', 'z', 'x') { foreach ($i in 'false', 'true') { foreach ($r in 'false', 'true') { foreach ($s in $speeds) {
    "    ""axis=$a,invert=$i,reverse=$r,speed=$s"": { ""model"": ""create:block/water_wheel""$($axisTurn[$a]) }"
} } } }
Write-File 'assets/create/blockstates/water_wheel.json' (Variants $lines)
Write-File 'assets/create/items/water_wheel.json' (Item-Json 'water_wheel')

# --- The three machines that work on the block in front ---------------------------------------
# They share the drill's casing and shape; only the head differs.
$bodyElement = Elem @(3, 3, 6) @(13, 13, 16) 'body'
$hubElement = Elem @(4, 4, 3) @(12, 12, 6) 'metal'
$drillShape = @(0.1875, 0.1875, 0, 0.8125, 0.8125, 1)
$machineShapes = Shapes-Json 'facing' $facings { param($f) , (Turn-Box $drillShape $f) }
$machines = @(
    @{ name = 'mechanical_saw'; texture = 'saw_blade'; key = 'blade'
       head = @((Elem @(1, 6, 1) @(15, 10, 3) 'blade'), (Elem @(6, 1, 1) @(10, 15, 3) 'blade')) },
    @{ name = 'mechanical_harvester'; texture = 'harvester_blade'; key = 'blade'
       head = @((Elem @(0, 7, 0) @(16, 9, 4) 'blade'), (Elem @(7, 0, 0) @(9, 16, 4) 'blade')) },
    @{ name = 'mechanical_plough'; texture = 'plough_share'; key = 'blade'
       head = @((Elem @(7, 11, 0) @(9, 15, 4) 'blade'), (Elem @(7, 1, 0) @(9, 5, 4) 'blade'),
                (Elem @(1, 7, 0) @(5, 9, 4) 'blade'), (Elem @(11, 7, 0) @(15, 9, 4) 'blade')) }
)
foreach ($m in $machines) {
    $name = $m.name
    "$($name):"
    $headTextures = [ordered]@{ metal = 'create:block/drill_metal'; blade = "create:block/$($m.texture)"; particle = 'create:block/drill_metal' }
    Write-File "assets/create/models/block/$($name)_head.json" (Model $headTextures (@($hubElement) + $m.head))
    $itemTextures = [ordered]@{ body = 'create:block/dark_casing'; metal = 'create:block/drill_metal'; blade = "create:block/$($m.texture)"; particle = 'create:block/dark_casing' }
    Write-File "assets/create/models/block/$($name)_item.json" (Model $itemTextures (@($bodyElement, $hubElement) + $m.head))
    $state = "$facingProp,`n    ""part"": { ""type"": ""enum"", ""values"": [""body"", ""head""] },`n$speedProp,`n$reverseProp"
    Write-File "data/create/horizon/block/$name.json" (Block-Json 'minecraft:stone' $state '"facing": "north", "part": "body", "speed": "0", "reverse": "false"' 3.0 6.0 $true 'minecraft:andesite' $machineShapes)
    $lines = foreach ($f in $facings) { foreach ($part in 'body', 'head') { foreach ($r in 'false', 'true') { foreach ($s in $speeds) {
        $model = if ($part -eq 'body') { 'mechanical_drill_body' } else { "$($name)_head" }
        "    ""facing=$f,part=$part,reverse=$r,speed=$s"": { ""model"": ""create:block/$model""$($facingTurn[$f]) }"
    } } } }
    Write-File "assets/create/blockstates/$name.json" (Variants $lines)
    Write-File "assets/create/items/$name.json" (Item-Json "$($name)_item")
}

"Mechanical Press:"
# Written for a shaft along z: a casing with two side posts, a shaft through it and a ram that
# the adapter lowers onto the block below. The ram's model never turns; the body and the shaft
# are turned to the other horizontal axis by the block state.
$pressBody = @(
    (Elem @(1, 10, 1) @(15, 16, 15) 'casing'),
    (Elem @(0, 4, 2) @(2, 10, 14) 'casing'),
    (Elem @(14, 4, 2) @(16, 10, 14) 'casing')
)
$pressShaft = @((Elem @(6, 11, 0) @(10, 15, 16) 'axle'))
$pressHead = @(
    (Elem @(7, 6, 7) @(9, 10, 9) 'metal'),
    (Elem @(3, 3, 3) @(13, 6, 13) 'metal')
)
$pressTextures = [ordered]@{ casing = 'create:block/dark_casing'; axle = 'create:block/steel'; metal = 'create:block/drill_metal'; particle = 'create:block/dark_casing' }
Write-File 'assets/create/models/block/mechanical_press_body.json' (Model $pressTextures $pressBody)
Write-File 'assets/create/models/block/mechanical_press_shaft.json' (Model $pressTextures $pressShaft)
Write-File 'assets/create/models/block/mechanical_press_head.json' (Model $pressTextures $pressHead)
Write-File 'assets/create/models/block/mechanical_press_item.json' (Model $pressTextures ($pressBody + $pressShaft + $pressHead))
# Boxes for a shaft along z, as fractions of a block; the x version swaps x and z.
$pressBoxes = @(@(0.0625, 0.625, 0.0625, 0.9375, 1, 0.9375), @(0, 0.25, 0.125, 0.125, 0.625, 0.875), @(0.875, 0.25, 0.125, 1, 0.625, 0.875))
$pressShapes = Shapes-Json 'axis' @('x', 'y', 'z') { param($a) $pressBoxes | ForEach-Object { if ($a -eq 'x') { , @($_[2], $_[1], $_[0], $_[5], $_[4], $_[3]) } else { , $_ } } }
$pressState = "    ""part"": { ""type"": ""enum"", ""values"": [""body"", ""shaft"", ""head""] },`n$speedProp,`n$reverseProp,`n    ""working"": { ""type"": ""boolean"" }"
Write-File 'data/create/horizon/block/mechanical_press.json' (Block-Json 'minecraft:oak_log' $pressState '"part": "body", "speed": "0", "reverse": "false", "working": "false"' 3.0 6.0 $true 'minecraft:andesite' $pressShapes)
$pressTurn = @{ z = ''; y = ''; x = ', "y": 90' }
$lines = foreach ($a in 'x', 'y', 'z') { foreach ($part in 'body', 'shaft', 'head') { foreach ($r in 'false', 'true') { foreach ($s in $speeds) { foreach ($w in 'false', 'true') {
    $turn = if ($part -eq 'head') { '' } else { $pressTurn[$a] }
    "    ""axis=$a,part=$part,reverse=$r,speed=$s,working=$w"": { ""model"": ""create:block/mechanical_press_$part""$turn }"
} } } } }
Write-File 'assets/create/blockstates/mechanical_press.json' (Variants $lines)
Write-File 'assets/create/items/mechanical_press.json' (Item-Json 'mechanical_press_item')

"Pressing recipes (a recipe type of the mod's own):"
function Pressing-Json([string]$ingredient, [string]$result, [int]$count, [int]$work) {
    "{`n  ""type"": ""create:pressing"",`n  ""ingredient"": ""$ingredient"",`n  ""result"": { ""id"": ""$result"", ""count"": $count },`n  ""work"": $work`n}`n"
}
Write-File 'data/create/recipe/pressing/iron_ingot.json' (Pressing-Json 'minecraft:iron_ingot' 'minecraft:iron_nugget' 9 300)
Write-File 'data/create/recipe/pressing/gold_ingot.json' (Pressing-Json 'minecraft:gold_ingot' 'minecraft:gold_nugget' 9 300)
Write-File 'data/create/recipe/pressing/sugar_cane.json' (Pressing-Json 'minecraft:sugar_cane' 'minecraft:paper' 1 200)

"Millstone:"
# Written for a shaft along z: a low basin with a ring of walls, two stone discs inside that
# turn about the vertical axis, and the shaft through its lower part.
$millBody = @(
    (Elem @(1, 0, 1) @(15, 5, 15) 'casing'),
    (Elem @(1, 5, 1) @(15, 12, 3) 'casing'),
    (Elem @(1, 5, 13) @(15, 12, 15) 'casing'),
    (Elem @(1, 5, 3) @(3, 12, 13) 'casing'),
    (Elem @(13, 5, 3) @(15, 12, 13) 'casing')
)
$millStones = @(
    (Elem @(3, 5, 3) @(13, 8, 13) 'stone'),
    (Elem @(4, 8, 4) @(12, 10, 12) 'stone'),
    (Elem @(7, 10, 7) @(9, 12, 9) 'metal')
)
$millShaft = @((Elem @(6, 1, 0) @(10, 5, 16) 'axle'))
$millTextures = [ordered]@{ casing = 'create:block/dark_casing'; stone = 'minecraft:block/stone'; metal = 'create:block/drill_metal'; axle = 'create:block/steel'; particle = 'create:block/dark_casing' }
Write-File 'assets/create/models/block/millstone_body.json' (Model $millTextures $millBody)
Write-File 'assets/create/models/block/millstone_stones.json' (Model $millTextures $millStones)
Write-File 'assets/create/models/block/millstone_shaft.json' (Model $millTextures $millShaft)
Write-File 'assets/create/models/block/millstone_item.json' (Model $millTextures ($millBody + $millStones + $millShaft))
$millBoxes = , @(0.0625, 0, 0.0625, 0.9375, 0.75, 0.9375)
$millShapes = Shapes-Json 'axis' @('x', 'y', 'z') { param($a) $millBoxes | ForEach-Object { , $_ } }
$millState = "    ""part"": { ""type"": ""enum"", ""values"": [""body"", ""stones"", ""shaft""] },`n$speedProp,`n$reverseProp"
Write-File 'data/create/horizon/block/millstone.json' (Block-Json 'minecraft:oak_log' $millState '"part": "body", "speed": "0", "reverse": "false"' 3.0 6.0 $true 'minecraft:andesite' $millShapes)
$lines = foreach ($a in 'x', 'y', 'z') { foreach ($part in 'body', 'stones', 'shaft') { foreach ($r in 'false', 'true') { foreach ($s in $speeds) {
    $turn = if ($part -eq 'stones') { '' } else { $pressTurn[$a] }
    "    ""axis=$a,part=$part,reverse=$r,speed=$s"": { ""model"": ""create:block/millstone_$part""$turn }"
} } } }
Write-File 'assets/create/blockstates/millstone.json' (Variants $lines)
Write-File 'assets/create/items/millstone.json' (Item-Json 'millstone_item')

"Milling recipes (a recipe type of the mod's own):"
function Milling-Json([string]$ingredient, [string]$result, [int]$count, [int]$work) {
    "{`n  ""type"": ""create:milling"",`n  ""ingredient"": ""$ingredient"",`n  ""result"": { ""id"": ""$result"", ""count"": $count },`n  ""work"": $work`n}`n"
}
Write-File 'data/create/recipe/milling/cobblestone.json' (Milling-Json 'minecraft:cobblestone' 'minecraft:gravel' 1 400)
Write-File 'data/create/recipe/milling/gravel.json' (Milling-Json 'minecraft:gravel' 'minecraft:sand' 1 400)
Write-File 'data/create/recipe/milling/bone.json' (Milling-Json 'minecraft:bone' 'minecraft:bone_meal' 3 300)
Write-File 'data/create/recipe/milling/blaze_rod.json' (Milling-Json 'minecraft:blaze_rod' 'minecraft:blaze_powder' 2 300)

"Belt:"
# Written for a shaft along z, which is the way the items go: two rails with a body between them.
# Belts chain end to end like shafts. The moving surface is drawn by the adapter, so the model has none.
$beltBody = @(
    (Elem @(1, 0, 0) @(3, 12, 16) 'casing'),
    (Elem @(3, 0, 0) @(13, 11, 16) 'casing'),
    (Elem @(13, 0, 0) @(15, 12, 16) 'casing'),
    (Elem @(6, 3, 0) @(10, 7, 1) 'axle'),
    (Elem @(6, 3, 15) @(10, 7, 16) 'axle')
)
$beltTextures = [ordered]@{ casing = 'create:block/dark_casing'; axle = 'create:block/steel'; surface = 'create:block/belt_surface'; particle = 'create:block/dark_casing' }
Write-File 'assets/create/models/block/belt_body.json' (Model $beltTextures $beltBody)
# The item's model has a surface of its own, since the block draws it moving.
Write-File 'assets/create/models/block/belt_item.json' (Model $beltTextures ($beltBody + @((Elem @(3, 11, 0) @(13, 11.2, 16) 'surface'))))
$beltShapes = "{`n    ""properties"": [""axis""],`n    ""values"": {`n      ""x"": [[0, 0, 0.0625, 1, 0.75, 0.9375]],`n      ""y"": [[0.0625, 0, 0, 0.9375, 0.75, 1]],`n      ""z"": [[0.0625, 0, 0, 0.9375, 0.75, 1]]`n    }`n  }"
$beltState = "$speedProp,`n$reverseProp"
Write-File 'data/create/horizon/block/belt.json' (Block-Json 'minecraft:oak_log' $beltState '"speed": "0", "reverse": "false"' 2.0 3.0 $true 'minecraft:andesite' $beltShapes)
$lines = foreach ($a in 'x', 'y', 'z') { foreach ($r in 'false', 'true') { foreach ($s in $speeds) {
    $turn = if ($a -eq 'x') { ', "y": 90' } else { '' }
    "    ""axis=$a,reverse=$r,speed=$s"": { ""model"": ""create:block/belt_body""$turn }"
} } }
Write-File 'assets/create/blockstates/belt.json' (Variants $lines)
Write-File 'assets/create/items/belt.json' (Item-Json 'belt_item')

"Loot tables and recipes:"
Write-File 'data/create/loot_table/blocks/belt.json' (Loot-Json 'belt')
Write-File 'data/create/recipe/belt.json' (Recipe-Json 'belt' @('create:shaft', 'minecraft:leather', 'minecraft:leather'))
Write-File 'data/create/loot_table/blocks/millstone.json' (Loot-Json 'millstone')
Write-File 'data/create/recipe/millstone.json' (Recipe-Json 'millstone' @('create:large_cogwheel', 'minecraft:cobblestone', 'minecraft:cobblestone'))
Write-File 'data/create/loot_table/blocks/mechanical_press.json' (Loot-Json 'mechanical_press')
Write-File 'data/create/recipe/mechanical_press.json' (Recipe-Json 'mechanical_press' @('create:cogwheel', 'minecraft:iron_ingot', 'minecraft:piston'))
foreach ($name in 'hand_crank', 'water_wheel', 'mechanical_saw', 'mechanical_harvester', 'mechanical_plough') {
    Write-File "data/create/loot_table/blocks/$name.json" (Loot-Json $name)
}
Write-File 'data/create/recipe/hand_crank.json' (Recipe-Json 'hand_crank' @('create:shaft', 'minecraft:stick', 'minecraft:stick'))
Write-File 'data/create/recipe/water_wheel.json' (Recipe-Json 'water_wheel' @('create:cogwheel', 'minecraft:oak_planks', 'minecraft:oak_planks', 'minecraft:oak_planks', 'minecraft:oak_planks'))
Write-File 'data/create/recipe/mechanical_saw.json' (Recipe-Json 'mechanical_saw' @('create:cogwheel', 'minecraft:iron_axe', 'minecraft:iron_ingot'))
Write-File 'data/create/recipe/mechanical_harvester.json' (Recipe-Json 'mechanical_harvester' @('create:cogwheel', 'minecraft:shears', 'minecraft:iron_ingot'))
Write-File 'data/create/recipe/mechanical_plough.json' (Recipe-Json 'mechanical_plough' @('create:cogwheel', 'minecraft:iron_hoe', 'minecraft:iron_ingot'))
"done"
