# Getting started

Build the [Mac app](../clients/macos/README.md) or
[Linux app](../clients/linux/README.md), then launch Kodosi. Public app releases are
not available yet.

## Sign in and open a terminal

Follow the app's sign-in flow in your browser. When adding a device to an existing
account, approve its code from a device you already use.

Create a terminal and choose its working folder. It is a real shell on that computer:
run your normal commands, editor, or coding agent there. Provider CLIs use their own
installation, settings, and sign-in.

Approved devices on the same account can open your terminals. The hosting computer
must remain online with Kodosi running.

## Work together

1. Create a room from Rooms.
2. Add the people you want to work with. Use People to connect with a friend first
   if they are not already in your list.
3. Start a terminal in the room, or share an existing terminal with it.
4. Use the room conversation and tasks alongside the terminals.

Everyone in the room gets full control of a shared terminal: typing, resizing,
interrupting, and closing. They can share their own terminals as well. People who
join later get access to the room's shared terminals and earlier conversation.

A room can span several computers, folders, and repositories. Files stay on each
terminal's host. Sharing one terminal does not share the host's other terminals.

## Bring an agent into the conversation

Start your agent in a room terminal and give it the bundled room skill:

```sh
kodosi room skill
```

The skill explains how to read and post messages, discover the room's terminals,
and create or claim tasks. Use your harness's normal way of loading skills or
instructions. See [Agents and tasks](AGENTS_AND_TASKS.md) for examples and CLI setup.

## See what a terminal needs

A terminal shows when its program is working, needs you, is done, or failed, with
the program's own message, such as the approval it asks for. Everyone who can open
the terminal sees the same state, and `kodosi session list` shows it too.

| You see | Meaning |
| --- | --- |
| A warm rim that turns round the terminal's mark | The program works. An arc and a number show its percent. |
| A copper ring, with a hand, a question mark, or a key | The program waits for your approval, your answer, or your sign-in. |
| A green check | The program is done. |
| A yellow warning sign | The program failed. |

The check and the warning sign go away when you open the terminal. The sidebar also
says how many terminals wait for an answer, and takes you to the next one.

While Kodosi is not the front app, your own terminals also send a system notification
when a program starts to wait, is done, or failed. Select the notification to open the
terminal. Your system's notification settings control these notifications.

Kodosi takes this from what the program reports to its terminal: the
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status)
(OSC 7501) and terminal progress bars (OSC 9;4). Claude Code reports its state from
version 2.1.295. Kodosi does not read the screen to guess; a program that reports
nothing shows no state.

## Keep work running

| Action | Result |
| --- | --- |
| Minimize a terminal | Hides its view; the process keeps running. |
| Close a terminal | Ends that terminal and its programs. |
| Close the app window | Leaves the app hosting its terminals. |
| Quit the app | Ends terminals hosted by that app. Other hosts keep running. |
| Temporarily lose a connection | The view reconnects to current terminal state when the host is reachable. |

If Kodosi reports uncertain input delivery, inspect the terminal before repeating a
command. It does not replay input that might already have run.

## If something does not connect

Check that the host is online, Kodosi is running there, and the terminal is still
shared with the room. A new device may need approval; a friend's changed identity
may need verification in People. For your own server, check the
[self-hosting guide](SELF-HOSTING.md).

[All documentation](README.md) · [Agents and tasks](AGENTS_AND_TASKS.md)
