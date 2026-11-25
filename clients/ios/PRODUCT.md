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
into the terminal that needs you, even from a cold launch.

### Terminal input and sizing
You can type directly into the terminal (with an accessory row for keys missing from the iOS
keyboard) or draft a longer reply in a composer that sends its text as a single paste. The phone
only resizes a terminal while that terminal has focus.

### Screen and chat views
Terminals with chat-capable agents can toggle between the terminal screen (with the accessory key
row) and the chat view. Each terminal opens in whichever view its agent runs in, and you can
switch views at any time.

### Full-screen canvas
The canvas opens full screen on demand and never interrupts you automatically. When an agent shows
or updates something, a banner names the item so you can tap to open it.

### Remote Dosi
Dosi runs on your computers. You pick which of your computers to message, and that machine's Dosi
handles the turn.

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
