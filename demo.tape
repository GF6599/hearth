# hearth demo, recorded with vhs (https://github.com/charmbracelet/vhs)
# Usage: just demo

Output doc/demo.gif

Set FontSize 14
Set Width 900
Set Height 506
Set Theme "Rose Pine"
Set Framerate 10
Set TypingSpeed 40ms

# A fixed seed draws the same frames on every recording.
Hide
Type "./target/release/hearth --seed 7"
Enter
Sleep 500ms
Show

Sleep 2s
Down
Sleep 600ms
Enter

# matrix
Sleep 4s
Tab
# fire
Sleep 4s
Tab
# life
Sleep 4s
Tab
# pipes
Sleep 5s
Tab
# bonsai
Sleep 7s
Tab
# aquarium
Sleep 5s
Tab
# typing
Sleep 5s

Escape
Sleep 1500ms
Type "q"
Sleep 300ms
