# Getting started

Build the app for your [Mac](../clients/macos/README.md),
[Linux or Windows](../clients/qt/README.md) computer, or your
[iPhone or iPad](../clients/ios/README.md). It needs a Kodosi backend and an OIDC
sign-in service; for development run a [local backend](DEVELOPMENT.md#run-the-backend).
Sign in through your browser. `kodosi auth login` shows a code to enter in any browser.

## Terminals

A terminal is a real shell on the computer that runs it. The sidebar lists your rooms
and terminals by folder and computer.

| Action | Result |
| --- | --- |
| Minimize | Hides the view. The terminal keeps running. |
| Close | Ends the terminal and its programs. |
| Close the window | The app keeps hosting its terminals. |
| Quit | Ends the terminals that this app hosts. Other computers keep theirs. |

After a lost connection the view returns to the current state. If Kodosi reports
uncertain input delivery, look at the terminal before you repeat a command.

## Agents

New terminal (⌘N on a Mac, Ctrl+Shift+N on Linux and Windows) opens the launcher. Pick
an agent, its folder, the account by its e-mail, and with the dots a model and an
effort. Type a first prompt and press Return: the new terminal's shell types the
command. A missing agent offers its install command. A new account gets an empty
sign-in folder, such as `~/.claude-work`, and the agent asks you to sign in. Kodosi also
finds the sign-in folders that your shell gives an agent, for example
`alias cw='CLAUDE_CONFIG_DIR=~/.claude-work claude'`.

A terminal at its prompt shows a mark for each installed agent, which starts it with
your last choice.

For an agent's own copy of a repository, hover the repository folder in the sidebar,
select the branch button and name a branch: Kodosi starts a terminal in a new Git
worktree, and removes it when the terminal ends if nothing changed. Settings › Terminal
chooses where these folders go. From a shell:
`kodosi session start --directory <repository> --branch <name>`.

## Terminal state

| You see | Meaning |
| --- | --- |
| A warm rim turning round the mark | Working. An arc shows the percent. |
| A hand, question mark or key | Waiting for your approval, answer or sign-in. |
| A green check | Done. |
| A yellow warning sign | Failed. |

Everyone who can open a terminal sees its state, and `kodosi session list` shows it too.
The check and the warning go away when you open the terminal. Small faces show who else
has it open. While Kodosi is in the background, your own terminals and the ones you have
open send a notification when they start to wait, finish or fail. The state comes from
the [Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status)
(OSC 7501) and progress bars (OSC 9;4); a program that reports nothing shows no state.

## Devices

Sign in on the new device with the same account. It shows a code: enter it in Settings
› Devices on a device you already use. An iPhone or iPad joins the same way. A
terminal's host must be online with Kodosi running.

Make a recovery key in Settings › Devices and keep it safe. If you lose every device,
sign in on a new one and choose "Use my recovery key" to keep your friends and rooms.
Remove a lost device from another device, and make a new recovery key if it was stolen.

## Friends and sharing

In People, send your invite, or paste a friend's invite or username. An invite carries
its owner's identity, so Kodosi can verify the friend.

<p align="center"><img src="media/people.webp" width="760" alt="People with two verified friends."></p>

The share button in a terminal's header shares it with friends or rooms. Everyone you
share with gets full control, with your privileges on that computer. Your other
terminals stay private.

## Rooms

Create a room in Rooms, add friends, then start a terminal in the room or share one into
it. Members can use every shared terminal and share their own. People who join later
get the terminals and the whole conversation. Files stay on each terminal's host.

Put tasks up for grabs, claim them, hand them back, close them with a result, or reopen
them. A room can link GitHub or Gitea repositories and turn issues into tasks; changing
such a task updates its issue.

## Agents in rooms

Agents in Kodosi terminals find the `kodosi` command. Give your agent the room skill
through its usual way of loading skills:

```sh
kodosi room skill
```

The [skill](../runtime/skills/kodosi-room/SKILL.md) covers the conversation, terminals,
tasks and repositories. In a Kodosi terminal the command finds the room itself;
elsewhere pass `--room <name>`.

GitHub actions use your `gh` sign-in or Git credential helper. Gitea uses the credential
helper, or `KODOSI_GITEA_TOKEN` for the origin in `KODOSI_GITEA_URL`. Credentials never
leave your computer.

## If something does not connect

Check that the host is online with Kodosi running and that the terminal is still shared.
A new device may need approval, and a friend with a changed identity may need
verification in People.
