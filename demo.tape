# hearth demo, recorded with vhs (https://github.com/charmbracelet/vhs)
# Usage: just demo

Output doc/demo.gif

Set FontSize 14
Set Width 900
Set Height 506
Set Theme "Rose Pine"
Set Framerate 8
Set TypingSpeed 40ms

# A fixed seed draws the same frames on every recording.
Hide
Type "./target/release/hearth --seed 7"
Enter
Sleep 500ms
Show

Sleep 2s
Enter

# embers
Sleep 3000ms
Tab
# ink
Sleep 3500ms
Tab
# tide
Sleep 3000ms
Tab
# constellations
Sleep 4000ms
Tab
# solar
Sleep 3500ms
Tab
# drive
Sleep 3500ms
Tab
# city
Sleep 3500ms
Tab
# river
Sleep 3500ms
Tab
# matrix
Sleep 2000ms
Tab
# fire
Sleep 2000ms
Tab
# life
Sleep 2000ms
Tab
# pipes
Sleep 2500ms
Tab
# bonsai
Sleep 3500ms
Tab
# aquarium
Sleep 2500ms
Tab
# murmuration
Sleep 2500ms
Tab
# hourglass
Sleep 2500ms
Tab
# study
Sleep 3500ms
Tab
# typing
Sleep 2500ms

Escape
Sleep 1500ms
Type "q"
Sleep 300ms
