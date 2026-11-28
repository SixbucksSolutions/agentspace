# The dry runs' stand-in for scripts\live\mask-proof.ps1: answers as it does on a pass, or as a planted failure.
# RFC 2119 keywords; the capitals are load-bearing. S01_PLANT: maskproof-fail (a FAILED line, exit 1), maskproof-false
# (exit 0 but an environment line reads False), maskproof-nosummary (exit 0, no summary line).
$plant = [string]$env:S01_PLANT
if ($plant -ceq 'maskproof-fail') { 'FAILED: a planted case'; 'Mask proof: 290 passed, 1 failed.'; exit 1 }
if ($plant -ceq 'maskproof-nosummary') { 'COMPUTERNAME removed: True'; 'USERNAME removed: True'; 'USERPROFILE removed: True'; 'USERPROFILE short form removed: True'; exit 0 }
'Mask proof: 291 passed, 0 failed.'
'COMPUTERNAME removed: True'
'USERNAME removed: True'
'USERPROFILE removed: True'
'USERPROFILE short form removed: ' + $(if ($plant -ceq 'maskproof-false') { 'False' } else { 'True' })
exit 0
