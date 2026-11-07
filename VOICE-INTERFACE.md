# The voice service interface

The reader links no speech engine and no audio library. It talks to the shared Trindle voice service over a Unix socket, and the service owns all sound, including the reader's short sound cues.

## Socket

`$XDG_RUNTIME_DIR/trindle-voice/say.sock`, an ordinary Unix stream socket. `TRINDLE_VOICE_SOCKET` names a different path. With no runtime folder there is no socket path, because a shared folder such as `/tmp` would let another user listen in.

A message is a command word on the first line, then `key=value` lines, then a blank line. Newlines inside spoken text are turned into spaces, so a message can never carry a second command.

## Messages from the reader

```
SAY
id=17
priority=focus
rate=60
volume=80
text=Save, button

```

- `id` is the reader's own counter.
- `priority` is `urgent`, `focus`, `normal` or `background`. `urgent` cuts everything. `focus` cancels all lower speech and clears the queue. `normal` queues. `background` speaks only when idle and may be dropped if it waits too long.
- `rate` and `volume` are 0 to 100, from the person's settings, and are left out when unknown.
- `text` is what to say.

```
STOP

```

Cancels everything the reader queued, now.

```
CUE
path=/home/example/.local/share/trindle-screen-reader/cues/dialog.wav
volume=60

```

Plays a short sound file by its path. A path with a line end in it is never sent.

## Messages from the service

One line each:

```
STARTED id=17
DONE id=17
CANCELLED id=17
ERROR id=17 reason=...
```

`STARTED` says sound has begun. `DONE`, `CANCELLED` and `ERROR` all mean the reader may send the next piece. An `ERROR` is written to the reader's log. A closed socket means the voice is lost: the reader prints its words to the terminal instead, plays a low double tone every 30 seconds, and reconnects with a growing delay.

## What the reader expects

- A new `focus` or `urgent` message ends the old speech at once, with no click.
- A stored or very short phrase starts within a few milliseconds, and a new focus change is heard within 300 ms.
- The service stays loaded while the reader is connected.

## Testing

The reader's tests use a fake service that listens on the same socket, records every message and answers `STARTED` and `DONE` from a script. The real service is never needed to run them.
