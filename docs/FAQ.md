# Questions and answers

Short answers, with a link to the page that has the details.

## The app

**Can I download Kodosi?**
Not yet. There are no published app releases. Build the
[Mac app](../clients/macos/README.md) or the [Linux app](../clients/linux/README.md)
from this repository.

**Which computers does it run on?**
A Mac with Apple silicon and macOS 26.4 or later, and Linux x86-64. Ubuntu 24.04 is
the recommended start for Linux.

**Is a terminal in Kodosi a real terminal?**
Yes. It is a real shell process on the computer that started it, drawn by
[Ghostty](https://ghostty.org). Run your normal commands, editor, or agent there.

**What happens to my terminals when I close the window?**
Closing the window leaves the app hosting its terminals. Minimize hides one view.
Close ends that terminal. Quit ends the terminals hosted by that app; terminals on
other computers keep running. See [Keep work running](GETTING_STARTED.md#keep-work-running).

## Agents

**Does Kodosi run my agent?**
No. Your agent runs through its own CLI in a terminal. Its execution, permissions,
settings, memory, sign-in, and conversation history stay with the provider. Kodosi
gives it a room to work in.

**Which agents can I use?**
Any agent that runs in a terminal, for example Claude Code, Codex, or Copilot CLI.

**Why does my agent show no state?**
Kodosi shows only the state that a program reports through the Program Status
Protocol (OSC 7501) or a terminal progress bar (OSC 9;4). Claude Code reports its
state from version 2.1.295. A program that reports nothing shows no state. See
[See what a terminal needs](GETTING_STARTED.md#see-what-a-terminal-needs).

**How does an agent read and write in a room?**
Through the `kodosi room` command and the bundled room skill. See
[Agents and tasks](AGENTS_AND_TASKS.md).

**Does a branch folder delete my work?**
No. Kodosi removes the folder and its branch only when nothing changed: no commit, no
changed file, no new file. A folder with work stays until you remove it with Git.

## Sharing

**What can someone do with a terminal that I share?**
Everything you can do there: type, resize, interrupt, and close, with your user's
privileges on that computer. This is intentional. Share a terminal only with people
you trust with that control.

**Does sharing one terminal share my computer?**
No. It shares that terminal; your other terminals are not shared. The terminal is a
shell on your computer, so a person in it can do there what you can do.

**Do the people in a room need the same repository or folder?**
No. A room can hold terminals from many computers, folders, and repositories.

**Do people who join a room later see the earlier conversation?**
Yes. New members get the room's shared terminals and its conversation history.

## Security and service

**What is encrypted?**
Terminal traffic and room content (messages, tasks, repository links) are encrypted
end to end. Private keys stay on your devices. See [Security](SECURITY.md).

**What can the service see?**
Account and device records, room membership, room and terminal metadata, content
routing fields, timing, and the sizes of encrypted data. It cannot read terminal or
room plaintext.

**Was the security design audited?**
No. The [security page](SECURITY.md) describes the implementation; it is not a claim
of an independent audit.

**Can I use my own server?**
Yes. Run the backend with PostgreSQL and your own OIDC provider, and start the apps
with two environment settings. There is no server selector in the apps yet. See
[Self-hosting](SELF-HOSTING.md).

**Where do my GitHub or Gitea credentials go?**
Nowhere. They stay on your computer; the Kodosi backend does not receive them.

## Help

**Something does not connect. What do I check?**
See [If something does not connect](GETTING_STARTED.md#if-something-does-not-connect).

**Where do I report a bug or a vulnerability?**
Bugs and ideas go to [GitHub issues](https://github.com/johnkozaris/Kodosi/issues).
Report a vulnerability [in private](SECURITY.md#report-a-vulnerability).

[All documentation](README.md) · [Getting started](GETTING_STARTED.md)
