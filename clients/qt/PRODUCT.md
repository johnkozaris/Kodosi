# Product

<!-- impeccable:product-schema 1 -->

## Platform

Linux desktop

## Purpose

Kodosi brings people and coding agents together through room-shared terminals and
conversation. Shared behavior and current implementation live in
[Kodosi's PRODUCT.md](../../PRODUCT.md).

## Principles

- Keep terminals and the room conversation easy to reach.
- Make common actions immediate; reveal secondary choices only when needed.
- Use plain language and native desktop behavior.
- Make access and control clear.
- Preserve files, provider history, identity, and user consent.
- Share a terminal with the room once; every member has full control, including
  people invited later.

## Scope

Make joining a room, contributing terminals from different machines and folders,
and participating in the conversation straightforward. Agents use simple room tools
and skills from their existing harnesses. They choose how to act on messages, tasks,
and handoffs.

Conversation stays beside the room's terminals, tasks, or connected repositories.
Mention a person or a room terminal with "@"; a terminal mention opens that terminal.
A message can become a task. Keep drafts and reading position when switching rooms or
work. Agents use the shared runtime's room CLI and bundled skill.

## Words

Visible text uses one word for one thing: terminal, room, friend, agent, device.
Resume is the word for saved provider conversations. Session, host, runtime, and
mission are internal words.

## Terminals

New Terminal opens a shell right away. The source list groups terminals by working
folder and shows which computer runs a remote terminal. With no terminal open, the
main area shows every terminal by computer. Go to (Ctrl+K) finds a terminal, a room,
a person, or an action. Rename a terminal in place.

A terminal carries the mark of the program in it. While an agent works, its mark and
title show it. A program that waits for you puts a copper ring on its mark and a sign
of what it needs beside it, and the source list offers the next terminal that waits
for an answer. A terminal that is done, that failed, or that rang its bell keeps a
sign until you look at it. While Kodosi is not in front, your own terminals send a
system notification for these changes.

A terminal at its prompt offers the start commands of this computer as marks among its
actions. Kodosi adds a command for an agent the first time that the agent runs here.
Settings holds the list, so a second account is one more command. One share sheet puts a terminal in a room or shares it with
friends.

The folder of a Git repository in the sidebar offers a terminal on a new branch. You
give the branch a name; it gets its own folder and a terminal starts there. A folder
with no changes goes away when its terminal ends.

## Experience

Kodosi should feel like a warm, living workbench, not an administration console.
Show the objects of the job, with short text, before explanations. Motion follows an
action or a real change of state, and never moves the terminal surface itself.
Keyboard access, screen-reader support, visible focus, sufficient contrast, and
reduced motion are part of the experience. [DESIGN.md](DESIGN.md) describes the
visual system, which the Mac client shares.
