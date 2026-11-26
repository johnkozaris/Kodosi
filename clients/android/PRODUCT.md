# Kodosi for Android

See [PRODUCT.md](../../PRODUCT.md) for shared product decisions and [DESIGN.md](DESIGN.md) for
the visual system.

The Android app watches, answers, and controls terminals running on your computers or shared by
friends, launches new terminals on your computers, and joins projects. Starting an agent inside an
existing idle terminal and viewing provider conversation history remain desktop-only. Requires
Android 15 or newer.

## Sign-in and device linking

Sign-in opens in a browser Auth Tab (falling back to a Custom Tab if unsupported) and returns to
the app automatically. A new phone or tablet can be approved by scanning a computer's QR code,
entering or scanning a verification code on an existing approved device, entering a recovery key,
or starting fresh. Scanning with the system camera can only start enrolling a new device; approving
another device always starts inside Kodosi. Connection failures explain the cause and offer Retry.

## Terminals

The **Terminals** tab groups terminals by computer and folder (your own computers first, then each
friend's), displaying each agent's icon and status indicator. The tab badge counts waiting
terminals you can answer. Tapping the name of one of your computers opens its page.

Tapping `+` launches a shell or agent on any of your online computers running Kodosi, letting you
pick account, model, effort, fast mode, permissions, verbosity, folder, and initial prompt. You can
also launch directly inside a project, and the new terminal opens once it starts.

Inside a terminal, the phone only sets terminal dimensions while focused, clamping to at least 10
rows. When screen space is tighter, the viewport follows the bottom rows (or tracks down to the
cursor when it sits higher up).

You can type directly into the terminal (with an accessory row for extra keys) or use the composer
to send a paste plus Return on tap. While an agent is waiting, quick-answer buttons `1`, `2`, and
`3` respond without bringing up the soft keyboard, and an agent that a restart stopped in the middle
of a task goes on with one tap. Hardware keyboards type directly into the
terminal, and landscape mode hides the header while typing.

Terminals with chat-capable agents support toggling between the terminal screen and chat view in
the header, opening on whichever view the agent uses. Dragging scrolls the terminal (or sends
scroll-wheel events when a program enables mouse reporting). Text selection, copy/paste (including
images), shared link rules, header renaming, live agent setting changes (with a notice when
queued for the next turn), and Android's predictive back gesture are all supported. In the chat
view, a long press on a message or a finished agent answer offers to copy it as written, to
select part of it, or to read it aloud in the voice of its language.

In the chat view, the mic key lets you talk instead of typing. Your words appear as your next
message while you speak, and the message sends after a long pause, so you can stop to think; Send
sends it without waiting for the pause. While the agent asks a question, only Send answers it with
your words. Tap the words to change them, or switch back to the keyboard: words that did not go out
stay in the composer. Speech turns into text on the device, so no audio leaves it, and replies stay
text. A device that can't turn speech into text in any of your languages shows no mic key.

From a terminal's menu, its owner shares it with friends or with a project, and everyone else sees
who shared it with them. A change that lets someone new in waits for the owner's OK in a dialog,
one question at a time; when the owner says no, the change ends without a message.

## Canvas

The canvas opens full screen only when requested. New or updated agent items display a banner with
the item name and open on tap, without interrupting your view. While open, the canvas follows
explicit agent `show` events rather than background file edits, and closed panels stay hidden on
this device until updated.

Files open as tabs or cards, with support for adding files via the system file picker, saving a
copy, removing items, or asking the agent about selected content. Documents, spreadsheets,
presentations, PDFs, and text files edit live, images support zoom, and the shared browser
provides tabs, history, an address bar, named remote cursors, keyboard input, and downloads.

## Projects, friends, and Dosi

Projects combine chat, tasks, linked repositories, members, and terminals. You can start a terminal
in a project or add one of your own from any computer. Wide screens show the chat alongside tasks,
repositories, or terminals.

**Friends** (opened from Settings, with a badge for pending requests) manages friend requests,
verified friends, trust verification, and blocked users. It generates QR codes for your invite and
scans friends' QR codes completely on-device without Google Play services. Sending or accepting a
request, verifying a friend, and trusting a changed identity each ask for your OK first. You can
block or report users from their profile, request, project member list, or chat message.

**Dosi** provides your synced conversation, choice prompts, and target-computer picker (any online
computer, or the one that answered last, which is the default). Messages for an offline computer
queue until it returns, and you can stop a turn, cancel a queued message, or start a new chat.

## Settings, notifications, and security

**Settings** covers linked devices (each of your computers opens a page to wake it, test its wake,
put it to sleep and choose when it stays awake), recovery key management, theme, terminal font size,
whether terminal lists lead with the agent summary before the terminal name, keep-screen-on,
document viewers, notifications, app lock, language, open-source licenses, legal/support links,
sign-out, and account deletion.

Push notifications fire when none of your devices are active. Terminal and computer names come
from a local bounded cache (falling back to generic labels when uncached). Waiting alerts use a
high-priority channel while done/failed alerts use a quiet channel, grouped per computer. Tapping
a notification opens that terminal after sign-in and app lock, and viewing a terminal on any
device clears its notification automatically.

App lock prompts for biometrics or device credentials on launch and after one minute away, hides
app content in the recent-apps switcher, and protects background content during screen sharing.
Device keys and auth credentials use authenticated encryption backed by Android Keystore, and app
data is excluded from Android backups and device transfers.

The app respects either the system language or its own per-app language setting, uses bottom
navigation on phones and a navigation rail on wide screens, and honors Android accessibility text
and touch-target settings (while terminal font size keeps its own control).
