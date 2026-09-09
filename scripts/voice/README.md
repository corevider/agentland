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

| model | size | good for | a short sentence, guessing | language named |
| --- | --- | --- | --- | --- |
| `tiny` | 75 MB | English at a pinch, nothing subtle | 0.70 s | 0.40 s |
| `base.en` | 142 MB | English only, fastest | 1.28 s | 0.66 s |
| `small` | 466 MB | many languages, including Turkish | 4.19 s | 2.51 s |
| `medium` | 1.5 GB | noticeably better Turkish, slower | — | — |

Models ending in `.en` hear English and nothing else.

Measured on a four-core i7-7700 against the same 1.4-second recording, with the
model already loaded. The two columns are the same work with and without the
guess in the next section: guessing the language is a second pass over the
audio, and on `small` it is the larger half of the wait.

## Which language

Left to guess, a model hears a short sentence as English — three Turkish words
came back as English ones. Name it in front of the command:

```
AGENTLAND_WHISPER_LANGUAGE=tr ~/.local/bin/agentland-transcribe {file}
```

Unset, it guesses, which is right for somebody who moves between languages and
wrong for short sentences. The guess is not free: it runs the model over the
audio once to decide, and then again to hear it. On `small` that is 4.19 seconds
against 2.51 — so anybody who dictates in one language only should name it, and
anybody who does not should know what the choice is buying.

## Why it stays running

Loading the model costs about eight seconds, and dictation that waits eight
seconds for every sentence is dictation nobody uses. The first call starts a
small process that holds the model, warms everything lazy in it, and answers
over a socket; it lets itself go after half an hour of silence.

`AGENTLAND_WHISPER_THREADS` sets how many threads it decodes on. Left alone it
takes one per core rather than one per hyperthread: two threads sharing a core's
arithmetic finish later than one, and the same sentence took 4.05 seconds on
eight threads and 3.21 on four. The machine has a crew running on it besides.

## Why it is loaded before you stop talking

Half an hour of silence and the model is gone, so the next sentence pays for it
again — and it used to pay at the worst moment, after the sentence was finished
and somebody was watching. Agentland now nudges the transcriber when the button
goes *down*: it hands it a second of silence, which for this script is the thing
that starts the process above, and the loading happens while the sentence is
still being said.

Measured on the same machine, cold, against a real recording:

| | a person waits |
| --- | --- |
| cold, nudged when the button went down | 3.8 s |
| cold, loading only once the sentence was over | 6.8 s |
| warm | 4.0 s |

The first row is the second one with the loading moved off the end. It needs a
sentence long enough to load under — about five seconds here — and a two-second
sentence gets some of it rather than all.

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
