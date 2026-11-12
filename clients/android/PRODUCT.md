# Kodosi for Android

[PRODUCT.md](../../PRODUCT.md) has the shared intent and [DESIGN.md](DESIGN.md) the
visual system.

The app follows, answers and controls the terminals of your computers and of the
friends who share with you, starts terminals on your computers, and takes part in
projects. Starting an agent in an open terminal, sharing a terminal with friends and
reading provider conversations stay on the computer. The app needs Android 15 or newer.

- Sign-in opens in an Auth Tab of the browser, with a Custom Tab where the browser has
  no Auth Tab. It comes back to the app when it ends. You approve a new phone or tablet
  as you approve a computer: with a code that you enter on an approved device, with the
  recovery key, or by starting fresh. A failed connection says why and offers Retry.
- The Terminals tab groups terminals by computer and folder: your computers first, then
  those of each friend. Each terminal has its agent's mark and the sign of its state.
  The tab counts the waiting terminals that you can answer.
- The + starts a shell or an agent on one of your computers where Kodosi runs. Pick
  its account, model, effort, permissions, verbosity, folder and first prompt. A project's terminals can start
  one in that project. The terminal opens when it starts.
- In a terminal, the phone sets the size only while it has focus, and never to fewer
  than 10 rows. With less room, the terminal shows its bottom rows, or the rows down to
  its cursor when the cursor is above them.
- Type in the terminal or in its composer, one at a time. The key row adds the keys a
  phone lacks. The composer sends a paste and Return only when you tap Send. While an
  agent waits, 1, 2 and 3 answer it without the keyboard. In landscape, the header
  hides while you type. A hardware keyboard types into the terminal.
- A drag scrolls the terminal, or the program when it uses the mouse. Links follow the
  shared link rules. Select and copy text, paste text or pictures, and rename your
  terminal from its header. Android's system back gesture leaves the terminal.
- The terminal's actions change the settings that its running agent can change with its
  own commands. A queued change says that it takes effect after the current turn.
- The canvas opens full screen when the person asks. New or changed content has a banner
  that names it and opens it on tap; it never takes over the screen by itself. An open
  canvas follows what the agent shows, not ordinary file changes.
  Its parts close on this device until they change. Files show as tabs or cards;
  add a file with the system picker, save a copy, remove it or ask the agent about it.
  Documents, sheets, presentations, PDFs and text files are live. Pictures can zoom.
  The shared browser has tabs, history, an address bar, named pointers, keyboard input
  and downloads.
- Projects hold conversations, tasks, repositories, members and terminals. On a wide
  screen the conversation stays beside the tasks, repositories or terminals. Friends,
  reached from Settings, holds requests, friends, trust checks and blocks. Its requests
  count on Settings.
- Friends shows your invite as a code and scans a friend's code with the scanner of
  Google Play services, without camera permission. A scanned or opened invite asks the
  trust question before it sends the request.
- Dosi has its shared conversation, questions and the computer that answers. Choose
  a computer that is online or the one that answered last. A message for a computer
  that is away waits for it; stop a turn, take back a queued message or start a new chat.
- Settings holds devices, the recovery key, appearance, terminal text size, whether the
  screen stays on, document viewers, notifications, app lock, language, licenses,
  sign-out and account deletion.
- Notifications name your terminal and computer from this device's cache, with generic
  words when they are unknown. Waiting has its own urgent channel. A tap opens the
  terminal after sign-in and app lock; opening it clears its notifications. Nothing
  shows while the app is in front, and Settings can turn notifications off.
- App lock uses the device's biometrics or screen lock when the app opens and after a
  minute away. It keeps the app's contents out of the app switcher and protects the
  background window during screen sharing. Device keys and sign-in credentials use
  authenticated encryption with a key in Android Keystore.
  App data stays out of backups and device transfers.
- The app follows the system language or its own language setting. Phones use bottom
  navigation and wide windows use a navigation rail. Text and touch targets follow
  Android's accessibility settings; terminal text has its own size setting.
