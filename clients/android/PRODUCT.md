# Kodosi for Android

[PRODUCT.md](../../PRODUCT.md) has the shared intent and [DESIGN.md](DESIGN.md) the
visual system.

The app follows and controls the terminals of your computers and of the friends who
share with you. Starting terminals or programs, sharing a terminal with friends and
reading provider conversations stay on the computer.

- Sign-in opens in a Custom Tab of the phone's browser, which comes back to the app when
  it ends. You approve a new phone as you approve a computer: with a code that you enter
  on an approved device, with the recovery key, or by starting fresh.
- The Terminals tab groups your terminals by computer, then the terminals of each friend,
  each with its mark and the sign of its state.
- In a terminal, the phone sets the size only while it has focus, and never to fewer
  than 10 rows. With less room, the terminal shows its bottom rows, or the rows down to
  its cursor when the cursor is above them.
- A tap on the terminal opens the keyboard, with a row of keys above it that adds esc,
  tab, ctrl and the arrows; ctrl holds for the next key. In landscape, the header hides
  while you type. A hardware keyboard types into the terminal. A drag scrolls the
  terminal's history. The back gesture leaves the terminal.
- The app's data stays out of backups and device transfers. The app follows the
  language of the phone, or the one that the phone's settings pick for Kodosi.
