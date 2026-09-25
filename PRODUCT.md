# Kodosi

Kodosi is a native terminal app for running terminals on your computer, using them
from your other approved devices, and sharing selected terminals with trusted friends.

## Product promise

- A terminal is a real process on the host computer.
- Approved own devices can use the owner's terminals.
- Friends can use only terminals explicitly shared with them.
- Everyone admitted to a terminal has full control, including input, resize, interrupt,
  and Close.
- Missions group people and terminals. Membership never grants terminal access.

Shared shell access has the host user's operating-system privileges. Kodosi is not a
sandbox and does not offer restricted terminal roles.

## A simple terminal experience

Starting, opening, minimizing, sharing, and closing a terminal should be easy to
understand without knowing how Kodosi works internally.

- Each screen should have one clear purpose.
- Show only the choices needed for the current task.
- Put uncommon details behind deliberate actions instead of presenting long forms,
  checklists, or explanatory walls.
- Use plain product language rather than protocol or architecture terms.
- Make the current state, consequences, and recovery path clear.

Minimize hides a terminal view and keeps its process running. Close ends the terminal.
Closing a window leaves the host running. Quit ends terminals hosted by that app, not
terminals on other computers.

Reconnecting should restore the current terminal without disrupting other viewers.
Kodosi must not repeat input when delivery is uncertain or let an old connection
control a replacement terminal.

## Sharing and trust

Friendship alone does not share anything. The owner chooses the friends for each
terminal and remains responsible for identity and sharing changes.

Removing a friend, device, or share blocks future access and queued delivery. It
cannot undo commands already run or data already received.

Private keys and terminal contents stay on endpoint devices. The service stores the
account, device, friendship, Mission, and sharing information needed to connect people
and route encrypted terminal traffic. It must not execute commands or receive terminal
plaintext.

## Provider support

Kodosi can preview saved Claude Code and Copilot CLI conversations and ask the
installed provider to resume one. It can also locate the provider's own configuration
files.

Provider prompts, permissions, settings, memory, and conversation storage remain
native to the provider. Kodosi does not install hooks, wrap provider commands, bypass
permissions, or build a second settings system.

## Product boundary

Kodosi does not provide:

- stopped-terminal archives;
- Mission chat, task boards, or agent dispatch;
- agent-intelligence dashboards or permission interception;
- provider plugin, skill, or memory management;
- restricted sharing roles or a project sandbox.

New ideas should make the terminal experience clearer. They should not add controls,
text, or concepts merely because the underlying systems expose them.
