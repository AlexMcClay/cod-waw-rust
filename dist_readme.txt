UNDEAD ROUNDS
=============

A fan-made, clean-room recreation of World at War's Zombies mode, written in
Rust. All game code is original. Art, sounds, weapon stats and the Nacht der
Untoten map are read at runtime from YOUR OWN copy of Call of Duty: World at
War. Nothing from the game is included here.

Running
-------
Double-click UndeadRounds.exe. The game looks for World at War in your Steam
libraries. If it says the install wasn't found, open undead.cfg and set
waw_path to your install folder (the one containing main\ and zone\).

Without an install you can still play "The Bunker (prototype)" map with
built-in sounds.

Controls
--------
WASD move, Shift sprint, Space jump, mouse look
C crouch, Ctrl or Z prone (Space or Shift stands back up)
Left mouse fire, right mouse aim, R reload, V or E knife
1 / 2 / Q / mouse wheel switch weapons
F interact (buy, open, use the box); hold F at a window to rebuild boards
Esc pause menu (resume, restart, options, quit)

Files
-----
undead.cfg     where your World at War install is
sounds.cfg     which game sounds play for which events
textures.cfg   which game textures the prototype bunker uses
Settings, log.txt and crash.txt are in %LOCALAPPDATA%\UndeadRounds
