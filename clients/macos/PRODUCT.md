# Kodosi for macOS

Shared product intent and current behavior live in
[Kodosi's PRODUCT.md](../../PRODUCT.md). This file describes the Mac experience.
[DESIGN.md](DESIGN.md) describes its visual system.

## Words

Visible text uses one word for one thing: terminal, room, friend, agent, device.
Resume is the word for saved provider conversations. Session, host, runtime, and
mission are internal words.

## Terminals

New Terminal opens a shell right away. The source list groups terminals by working
folder and shows which computer runs a remote terminal. With no terminal open, the
main area shows every terminal by computer. Go to (⌘K) finds a terminal, a room, a
person, or an action. Rename a terminal in place.

A terminal carries the mark of the program in it. While an agent works, its mark and
title show it. A program that waits for you puts a copper ring on its mark and a sign
of what it needs beside it, and the source list offers the next terminal that waits
for an answer. A terminal that is done, that failed, or that rang its bell keeps a
sign until you look at it. While Kodosi is not in front, your own terminals send a
system notification for these changes.

Minimize hides a terminal but keeps it running. Close ends it and its programs.
Closing the window leaves Kodosi running; Quit ends terminals that run on this Mac,
not on other computers. Hidden terminals keep their place without accepting
accidental input. Sizing should feel automatic, and keys like Ctrl-C should
retain their shell meaning. Terminals stay dark in the light appearance.

One share sheet puts a terminal in a room or shares it with friends.

## Rooms

Make it easy to join a room, contribute a terminal, open someone else's shared
terminal, and participate in the room conversation. Sharing with the room gives its
members full terminal control, including members invited later. Keep the hosting
computer and working folder clear without requiring a common folder or repository.

Conversation stays beside the room's terminals, tasks, or connected repositories.
Mention a person or a room terminal with "@"; a terminal mention opens that terminal.
A message can become a task. Keep drafts and reading position when switching rooms or
work. Agents use the shared runtime's room CLI and bundled skill.

## Providers

Preview saved Claude Code and Copilot CLI conversations without changing them.
Keep long histories readable.
Resume starts a new terminal through the provider's own CLI, with its usual
prompts and permissions. Kodosi opens original provider settings without
editing them.

## Mac experience

Keep the terminal central and the actions clear. Show the objects of the job, with
short text, before explanations. Use warm charcoal, cream, and copper. Motion follows
an action or a real change of state, and never moves the terminal surface itself.
Native focus, accessibility, localization, and reduced motion matter.
