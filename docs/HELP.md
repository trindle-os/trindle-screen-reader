# Trindle screen reader help

This page is for the person using the reader. It is short on purpose, and it is also written to be read aloud. The reader is a beta that blind testers have not yet used, so do not make it your only way into a computer yet.

## What it does

The reader speaks what is on the screen: which window you are in, which button or field you have reached, and what you type. It follows the cursor in a text box, can read a whole document aloud, and lets you look around a window without moving anything. It uses no network and sends nothing anywhere.

It is the one program that comes with Trindle already installed, so you can set everything else up. Turn it on or off in Settings, or with Super, Alt and S held together. The desktop does this, not the reader. When it starts you hear "Welcome". At the login screen you hear "Screen reader on", and it hands over to your own reader when you sign in, so you never hear two voices.

## The reader key

Almost every command is the reader key plus another key. The reader key is Caps Lock or Insert. Hold it, press the other key, and let go. The reader key and the keys pressed with it go to the reader only, so the program you are in never sees them.

Tap Caps Lock twice quickly to switch the real Caps Lock on or off. A single tap does nothing, so you never switch it by accident. The reader says "Caps lock on" or "Caps lock off" when it changes.

Press Control on its own to stop speech at once. Press Control while holding anything else and it does nothing.

## Everyday keys

Each of these is the reader key plus the key shown.

| Key | What it does |
|---|---|
| S | Turns speech off and on. |
| R | Says the last thing again. |
| Shift and R | Spells the last thing, letter by letter. |
| W | Says where you are. |
| P | Sends the next key to the program untouched. Use it when a program needs a key the reader would otherwise keep. |
| H | Learn mode. |
| A | Say all, on a laptop layout. |
| Right and Left arrows | During say all, skip to the next or previous sentence. |
| Down and Up arrows | During say all, skip to the next or previous paragraph. |

On a keyboard with a number pad, Plus on the pad starts say all.

Learn mode is for finding out what a key does without doing it. Press any key, with the reader key where it needs one, and the reader says what the command is called, or "not a command". Press Escape twice to leave.

## Looking around: the review layer

Review reads the screen as flat text and does not move your real cursor. Press the reader key with V, let go, then press one key at a time. Each key makes one move, then the layer closes. Press Escape to close it without doing anything.

| Key | What it does |
|---|---|
| Up and Down arrows | Previous and next line. |
| Left and Right arrows | Previous and next word. |
| Comma and full stop | Previous and next letter. |
| L, W, C | Read this line, word or letter. |
| S and E | Start and end of the line. |
| Home and End | First and last line. |
| T and B | Top left and bottom right. |
| A | Read on from here. |

With a number pad you do not need the layer. Keys 7, 4 and 1 are previous line, read line and next line. Keys 8, 5 and 2 are the same for words, and keys 9, 6 and 3 for letters. Hold the reader key with them.

## Browse mode

Browse mode moves a cursor through a window in reading order, like a page. Press the reader key with B, let go, then one key. The program's own focus follows the cursor onto buttons, links and fields, so pressing Enter then acts on what you just heard.

| Key | What it does |
|---|---|
| Down and Up arrows | Next and previous item. |
| R | Read where you are. |
| M | Switch between browse mode and focus mode. In focus mode your keys go to the field, so you can type. |
| 1, 2, 3 | List the headings, the links or the form fields. |
| H, K, L, T, F, B, G, A | Jump to the next heading, link, list, table, form field, button, graphic or landmark. |

Add Shift to any jump letter to go backwards. When there is nothing more the reader says, for example, "no next heading", "End of document" or "Start of document". Lists and tables are announced when you enter them, for example "list with 4 items". A window with nothing in it gives "nothing to browse".

When the desktop supports it, browse mode stays open after the reader key and B, so you press the keys above one after another. Escape leaves it, saying "browse off". Any reader key command also leaves it, and so does a text field, where your keys go to the field. If the desktop does not support it yet, the layer takes one key and closes, so press the reader key and B again for each move.

## Settings

Press the reader key with G, let go, then an arrow key.

| Key | What it does |
|---|---|
| Up and Down arrows | Faster and slower, in steps of 5. The reader says "Rate" and the number. |
| Right and Left arrows | Louder and quieter, in steps of 5. The reader says "Volume" and the number. |
| V | Switch between brief and verbose descriptions. |
| T | Go round the typing echo choices: off, characters, words, both. |
| S | Sounds on or off. |
| Page Up and Page Down | Louder and quieter sounds, in steps of 5. |
| L | Letters and punctuation for review (laptop) or the number pad. |

Each choice is saved at once and the reader says what it is now. The speed never goes below 10, so no setting can make the voice too slow to use.

The same settings are also in a plain file, `~/.config/trindle-screen-reader/settings.toml`, if you prefer to edit them. None of it is required.

| Setting | Values | What it does |
|---|---|---|
| `verbosity` | `brief`, `verbose` | How much is said about each thing. |
| `typing_echo` | `none`, `character`, `word`, `both` | What is spoken as you type. |
| `rate` | 10 to 100 | Speaking speed. |
| `volume` | 0 to 100 | Speech volume. |
| `sounds` | `true`, `false` | Short sounds for windows, alerts, notifications and secret fields. |
| `cue_volume` | 0 to 100 | Volume of those sounds, separate from speech. |
| `laptop_layout` | `true`, `false` | Letters instead of the number pad. |

If the file has a value the reader cannot use, it uses a safe one, says "Some settings could not be used, so safe ones are in place", and keeps the good values.

## The login screen

At the login screen the reader runs in its own mode, so you can pick your name and type your password. When you sign in, your own reader waits for the login one to finish and then takes over. This takes a few seconds at most. If the login reader is stuck, your reader starts anyway after five seconds, so you are never left in silence.

The lock screen is not covered yet.

## Privacy

The reader never speaks a password, never speaks the text of a secret field, and never reads it from the program. It names the field as a "password entry" and plays a short sound as you type, nothing more. In review, a secret field is read as "protected". It does not use the network. The only files it keeps are your settings, and the sound files and a short log in the folder `$XDG_RUNTIME_DIR/trindle-screen-reader`. The log is for faults only.

## If something goes wrong

**It says nothing.** Press the reader key with S, in case speech was turned off. You hear "Speech on" when it is back. If there is still no voice, the reader plays a low double tone every 30 seconds so you know it is running but cannot speak. The cause is the Trindle voice service not running. Restart the computer, or ask someone to look at the log.

**It said "Screen reader restarted".** The reader stopped and its watcher started it again. Nothing needs doing. If you hear it often, tell the Trindle community, with the log if you can.

**It says "Speech restarted".** The voice service went away and came back. Nothing needs doing.

**It says "that key did not come from the keyboard, so this command is not run".** A program tried to press a reader key for you. The reader refuses this on purpose, so no program can change your settings behind your back. Press the key yourself.

**A key does not work in one program.** Use the reader key with P and then press the key. It goes straight to the program.

**Two voices at once.** This should never happen. If it does, it is a bug and we want to hear about it.

**Some settings could not be used.** A line in the settings file is wrong. The good lines still work. Delete the file to go back to the defaults.

To report a problem, use the [Buildmorphic community](https://buildmorphic.com). To report a security problem privately, read [SECURITY.md](../SECURITY.md).
