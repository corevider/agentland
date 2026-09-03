# Hearing what you said

Agentland records with whatever is already on the machine and hands the file to
a command you name in Settings → Voice. Nothing is bundled: a microphone and a
speech model are both things to install on purpose.

This is one way to fill that slot, and the one measured on Linux.

## Install

```sh
python3 -m venv ~/.local/share/agentland-voice/venv
~/.local/share/agentland-voice/venv/bin/pip install faster-whisper

install -Dm755 scripts/voice/agentland-transcribe ~/.local/bin/agentland-transcribe
sed -i '1s|.*|#!'"$HOME"'/.local/share/agentland-voice/venv/bin/python|' ~/.local/bin/agentland-transcribe
```

Then in Settings → Voice:

```
~/.local/bin/agentland-transcribe {file}
```

The model downloads itself on first use, into
`~/.local/share/agentland-voice/models`, and stays there. Whisper's weights are
MIT-licensed: no account, no key, and nothing leaves the machine.

## Which model

`AGENTLAND_WHISPER_MODEL` picks it, `small` by default.

| model | size | good for |
| --- | --- | --- |
| `base.en` | 142 MB | English only, fastest |
| `small` | 466 MB | many languages, including Turkish |
| `medium` | 1.5 GB | noticeably better Turkish, slower |

Models ending in `.en` hear English and nothing else.

## Why it stays running

Loading the model costs about eight seconds, and dictation that waits eight
seconds for every sentence is dictation nobody uses. The first call starts a
small process that holds the model, warms everything lazy in it, and answers
over a socket; it lets itself go after half an hour of silence.

Measured on an eight-thread CPU with `small`: about 9.7 seconds for the first
sentence and 2.4 for every one after it.

## Speaking from a phone, or from Windows over a remote desktop

The machine running the crew often has no microphone — over a remote desktop
there is none to forward, and the phone in your hand has a better one. The
companion page at `/mobile` has a box for words and, where the browser will
allow a microphone, a button that records and sends the audio here to be read
back.

Two ways in, and the page offers whichever is available:

- **The keyboard's own dictation.** Tap the microphone on the phone keyboard, or
  press `Win+H` on Windows, and speak into the box. Nothing is recorded by the
  page, so no permission and no secure page is needed.
- **Hold to speak.** Only when the page is served over https or from localhost:
  browsers refuse a microphone on a plain http page, which is what a home
  network gives you. The recording is sent to `/voice/heard`, converted, read
  back by the same transcriber, and put in the box to be checked before sending.

Either way the words go where you choose: to an agent with a pane open, or —
choosing nobody — they become the project's goal, which survives restarts and
is handed to the commander every time it comes back.

To reach it from the phone, the core has to listen beyond this machine:

```sh
AGENTLAND_HOST=0.0.0.0 agentland-core
```

Then open `http://<this machine's address>:9470/mobile?token=<the token>` on the
phone; the token is in `service.json` under the data folder. The addresses this
machine answers on are allowed automatically. Everything stays on your network:
the audio goes to your own core and the model runs there.
