# hearth demo, recorded with vhs (https://github.com/charmbracelet/vhs)
# Usage: just demo

Output doc/demo.gif

Set FontSize 14
Set Width 1400
Set Height 800
Set Theme "Rose Pine"
Set Framerate 30
Set TypingSpeed 50ms

# A fixed seed draws the same frames on every recording.
Type "./target/release/hearth --seed 7"
Enter
Sleep 2s

Enter
Sleep 4s
Tab
Sleep 4s
Tab
Sleep 4s

Escape
Sleep 1500ms
Type "q"
Sleep 500ms
