# Print Windows dogfood checklist + exact commands for wake→click→G07.
# Does not download models or start the app (safe to run anytime).
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
if (-not $Root) { $Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path }
$Checklist = Join-Path $Root "docs\dogfood\2026-09-04-dogfood-checklist.md"

Write-Host "=== WorkDance Dogfood (Windows) ==="
Write-Host "repo: $Root"
Write-Host ""

if (Test-Path $Checklist) {
  Write-Host "--- checklist: docs/dogfood/2026-09-04-dogfood-checklist.md ---"
  Get-Content -LiteralPath $Checklist -Encoding UTF8
  Write-Host ""
}

$DataLocal = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { Join-Path $env:USERPROFILE "AppData\Local" }
$Hand = Join-Path $DataLocal "workdance\models\hand_landmarker.task"
$Asr = Join-Path $DataLocal "workdance\models\asr\paraformer-zh-small"

Write-Host "--- exact commands ---"
@"
# 1) Models (Git Bash / WSL — SHA256 pinned in scripts)
./scripts/download-hand-landmarker.sh
./scripts/download-asr-model.sh

# Expected paths (override with WORKDANCE_HAND_MODEL / WORKDANCE_MODEL_DIR / WORKDANCE_ASR_MODEL_DIR):
#   hand: $Hand
#   asr:  $Asr

# 2) Native MediaPipe DLL (dlopen — not linked at compile time)
#    `$env:MEDIAPIPE_LIB = "C:\path\mediapipe.dll"`

# 3) Run desktop with both features (do NOT set *_STUB=1)
cd apps\desktop
npm install
npm run tauri -- dev --features dogfood
# equivalent: --features mediapipe-hands,sherpa-asr

# 4) Permissions: Camera + Microphone; allow input injection
# 5) Confirm non-stub: Settings → vision backend mediapipe-hands; ASR sherpa-asr
#    G07 text must NOT be the fixed stub sentence 「实验记录已追加。」
"@

Write-Host ""
$handOk = Test-Path -LiteralPath $Hand
$asrOk = (Test-Path -LiteralPath (Join-Path $Asr "model.int8.onnx")) -and (Test-Path -LiteralPath (Join-Path $Asr "tokens.txt"))
Write-Host ("hand model present? {0} → {1}" -f ($(if ($handOk) {"YES"} else {"NO"}), $Hand))
Write-Host ("asr model ready?   {0} → {1}" -f ($(if ($asrOk) {"YES"} else {"NO"}), $Asr))
