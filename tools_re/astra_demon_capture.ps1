param([ValidateSet('claw','charge','follow','blade','judge','fire','lava','baseline')][string]$Scene = 'claw')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$out = Join-Path $root "target/astra_validation/$Scene"
New-Item -ItemType Directory -Force $out, "$out/costumes", "$out/fs" | Out-Null
Copy-Item -LiteralPath "$root/target/demon_probe/bench/01035D0B" -Destination "$out/fs" -Recurse -Force
$keys = '320:OK,420:OK,470:RIGHT,520:OK,660:1'
$frames = 1050
$extra = @()
switch ($Scene) {
    'claw' { $keys += ',800:OK,820:OK,840:OK,860:OK,880:OK' }
    'charge' { $extra = @('--h2-beast-skill','640:3:12:5','--h2-quickbar','650:3:13'); $keys += ',800:9,806:LEFT:70' }
    'follow' {
        $extra = @('--h2-beast-skill','640:1:2:5','--h2-quickbar','650:1:11')
        $keys += ',760:3'
        foreach ($f in 780..1280 | Where-Object { $_ % 15 -eq 0 }) { $keys += ",${f}:OK" }
        $keys += ',1320:3'
        $frames = 1450
    }
    'blade' { $extra = @('--h2-beast-skill','640:1:3:5','--h2-quickbar','650:1:11'); $keys += ',760:3' }
    'judge' { $extra = @('--h2-beast-skill','640:3:10:5','--h2-quickbar','650:3:13'); $keys += ',760:9' }
    'fire' { $extra = @('--h2-beast-skill','640:3:11:5','--h2-quickbar','650:3:13'); $keys += ',760:9' }
    'lava' { $extra = @('--h2-beast-skill','640:2:8:5','--h2-quickbar','650:2:12'); $keys += ',760:7' }
    'baseline' { $keys = '320:OK,420:OK,520:OK'; $frames = 1200 }
}
$dumps = @(if ($Scene -eq 'baseline') { 560; 1200 } else { 680..$frames | Where-Object { $_ % 3 -eq 0 } })
$argsList = @('--zip',"$root/android_app/app/src/main/assets/hero2_ktf.zip",'--fs',"$out/fs",
    '--w','176','--h','220','--ms','20','--frames',"$frames",'--keys',$keys,'--out',$out,
    '--dump',($dumps -join ','),'--fx-lib',"$root/wie_android/src/demon_fx.bin",
    '--h2-costume-dir',"$out/costumes",'--h2-beast-skill','640:0:9:5','--h2-quickbar','650:0:10',
    '--h2-mob-hp','705:30000') + $extra
$argsList | ConvertTo-Json | Set-Content -LiteralPath "$out/arguments.json" -Encoding utf8
$ErrorActionPreference = 'Continue'
& "$root/target/release/wie_probe.exe" @argsList *> "$out/probe.log"
if ($LASTEXITCODE -ne 0) { Get-Content "$out/probe.log" -Tail 40; throw "Probe $Scene failed" }
Get-Content "$out/probe.log" -Tail 5
