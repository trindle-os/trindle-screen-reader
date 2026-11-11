# Changelog

## 0.1.0 (7 November 2026)

The first public beta. It has not yet been used by blind testers.

- Fixes from the 11 November review: the session's reader opens the login hand-over lock read-only, never follows a link and never changes its mode (the login screen's reader makes the file, mode 0644); a state change (selected, busy, pressed and so on) is only spoken for the object with the focus; a key typed into a password field always clicks; review and browse say when a window was too large to read whole; paragraphs, quotes and terminals are read in browse and review; a compositor that reconnects replaces the one before it; the accessibility switch is only turned off if the reader turned it on; the accept loops log a failure and carry on; control characters are removed from program text before it goes to the voice; the plugin is built from the pinned toolkit, with a check for paths from the build computer; dead code removed.
- Settings layer (reader key then G): rate, volume, brief or verbose, typing echo, sounds, sound volume and laptop keys, all saved at once.
- Fixes from the 5 November review: say-all reads past 20,000 characters and says when the rest was left; a late answer from a slow program no longer overrides the current focus; a flood from one program is capped; stopping say-all before its text arrives now stops it; a failing sound player hands over to the next; a reconnecting compositor closes any open mode; reader key then P sends a `pass` line; browse mode with nothing to browse no longer panics.
- Lock screen events: the lock program sends locked, time, entry length, wrong and unlocked over events.sock (see LOCK-SOCKET.md).
- Speaks focus, window, dialog, alert, notification, value and name changes read from the accessibility bus.
- Follows the caret and echoes typing by character, by word or both.
- Where am I, repeat, spell last and history.
- Say all, with skip by sentence and by paragraph.
- Flat review by line, word and character.
- A first browse mode: reading-order cursor, jumps by structure including landmarks, element lists, browse and focus mode, and the program's own focus follows the cursor onto controls.
- Browse stays open for plain keys when the compositor offers them (state plainkeys, state browse on|off).
- Sound cues for windows, alerts, notifications and secret fields.
- Never speaks passwords, never uses the network, never talks over itself.
- Plays a low double tone and writes a log when the voice service is missing.
- Login-screen mode, with a lock file so two readers never run at once.
- Keys are trusted as real only from the compositor's own program.

Say all and review are tested with scripted sessions; gathering for review and browse mode was checked on a live Toolkit window. Not yet built: single-letter browse keys (they need the compositor), the lock screen, and braille.
