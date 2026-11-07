# Lock screen events

The lock screen tells the reader what is happening, so a blind person can unlock the computer.

- Socket: `$XDG_RUNTIME_DIR/trindle-screen-reader/events.sock` (made by the reader, mode 0600). Connect only if it exists; if the reader is off, send nothing.
- Who may connect: the same user, running `/usr/lib/trindle/bin/trindle` (the lock screen is that same file started as trindle-lock). Any other peer is refused. If the reader runs in a box and cannot see the peer's pid, a peer of the same user is accepted because the folder is mode 0700 and the user's own. After a package upgrade the old file shows as "(deleted)" and still counts.
- Lines, one per event, ending in a newline, at most 64 bytes:

| Line | The reader says |
|---|---|
| `locked` | Screen locked |
| `time HH:MM` | the time, for example 10:30 (two digits each, 24 hours) |
| `entry N` | N typed, or "nothing typed" for 0. N is the length only, 0 to 999; never send the text |
| `wrong` | Wrong password, try again |
| `unlocked` | Unlocked |

- A line that does not match exactly is dropped, and a line over 64 bytes closes the connection. The queue holds 16 events; more are dropped.
- Lines carry no free text, so the lock screen cannot make the reader say anything else, and a password can never reach the voice.
