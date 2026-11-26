# Kodosi for iPhone and iPad

See [PRODUCT.md](../../PRODUCT.md) for shared product decisions and [DESIGN.md](DESIGN.md) for
the visual system.

The iOS app lets you watch, answer, and control terminals running on your computers or shared by
friends, launch new terminals on your computers, and participate in projects. It never hosts
terminals locally. Starting an agent inside an existing idle terminal and browsing provider
conversation history stay desktop-only.

## Priorities and behavior

### Built for quick answers
The app is designed around agents waiting for input. Every terminal displays its status, common
short answers for waiting agents are one tap away, and tapping a push notification jumps straight
into the terminal that needs you, even from a cold launch. An agent that a restart stopped in the
middle of a task goes on with one tap too.

### Terminal input and sizing
You can type directly into the terminal (with an accessory row for keys missing from the iOS
keyboard) or draft a longer reply in a composer that sends its text as a single paste. The phone
only resizes a terminal while that terminal has focus.

### Screen and chat views
Terminals with chat-capable agents can toggle between the terminal screen (with the accessory key
row) and the chat view. Each terminal opens in whichever view its agent runs in, and you can
switch views at any time. In the chat, a long press on a message or a finished answer copies it
whole, as written, or opens it to select part of it.

### Talking in chat
In the chat view, the mic key lets you talk instead of typing. Your words appear as your next
message while you speak, and the message sends after a long pause, so you can stop to think; Send
sends it without waiting for the pause. While the agent asks a question, only Send answers it with
your words. Tap the words to change them, or switch back to the keyboard: words that did not go out
stay in the composer. Speech turns into text on the device, so no audio leaves it, and replies stay
text. A device that can't turn speech into text in any of your languages shows no mic key.

### Full-screen canvas
The canvas opens full screen on demand and never interrupts you automatically. When an agent shows
or updates something, a banner names the item so you can tap to open it.

### Remote Dosi
Dosi runs on your computers. You pick which of your computers to message, and that machine's Dosi
handles the turn.

### Waking a computer
Each of your computers has a page, opened from Settings or from its name in the Terminals list, to
wake it, test its wake, put it to sleep and choose when it stays awake. iOS lets only apps with a
special entitlement send the wake signal, so an iPhone or iPad never sends it. It asks one of your
awake computers on the network of the sleeping computer.

### In-person invites and QR device linking
Your friend invite can be shown as a QR code that a friend scans with the Camera app or inside
Kodosi. Kodosi asks before someone becomes a friend, so the scan sends the request only after they
confirm.

A new iPhone or iPad links in a single scan of a QR code shown on an approved computer, and an
approved phone can scan a new device's code to approve it. Scanning with the system Camera app can
only enroll a new device; approving another device always starts inside Kodosi.

### App lock
When enabled, app lock requires Face ID, Touch ID, or the device passcode and conceals app content
in the app switcher while backgrounded.
