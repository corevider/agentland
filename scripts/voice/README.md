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
