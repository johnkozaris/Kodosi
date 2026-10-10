# Pictures

The pictures in the README and the guide are captures of the Kodosi Mac app, built
from this repository. The interface is real. The data is sample data:

- The people (Maya Chen, Leo Park, Ana Ruiz), their devices, rooms, messages, and
  tasks are invented. They ran on a local backend with a disposable database.
- The terminal text comes from scripts that print sample agent output and report their
  state through the Program Status Protocol (OSC 7501), as a real agent does. No
  provider account, conversation, or repository was used.

| File | Shows |
| --- | --- |
| [banner.webp](banner.webp) | The Kodosi mark and line. Opening frame of the launch film. |
| [app-dark.webp](app-dark.webp), [app-light.webp](app-light.webp) | The window with four terminals on three computers, in both appearances. Terminals stay dark. |
| [overview.webp](overview.webp) | All terminals, grouped by computer. |
| [status.gif](status.gif), [status.webp](status.webp) | One terminal that works, waits, works, and is done. The still is for readers who ask for reduced motion. |
| [start.webp](start.webp), [start-settings.webp](start-settings.webp) | Start marks in a terminal header, and their commands in Settings. |
| [branch.webp](branch.webp) | The New branch panel. |
| [room.webp](room.webp), [tasks.webp](tasks.webp) | A room with its conversation, and its tasks. |
| [share.webp](share.webp) | The share panel of a terminal. |
| [people.webp](people.webp) | Friends in People. |

Window captures are 2640 × 1680 pixels, scaled to 2000 pixels wide. Detail pictures
are crops of the same captures. The animation has eight frames, 1264 pixels wide,
with a 200-color palette.

## Replace a picture

Use isolated sample data: a disposable database and a disposable `KODOSI_DATA_ROOT`,
as [isolated validation](../DEVELOPMENT.md#isolated-validation) describes. Do not
capture your own terminals, names, hosts, or paths. Keep the file name so the README
and the guide keep their links.

## Credits

Kodosi's logo, palette, and interface are project assets. Terminal text uses
JetBrains Mono, by the JetBrains Mono Project Authors, under the
[SIL Open Font License](../../terminal/ghostty/ThirdPartyNotices/licenses/jetbrains-mono-OFL-1.1.txt).

Claude and Codex glyphs use LobeHub artwork under the
[MIT license](../../clients/macos/Resources/Notices/LobeHub-Icons-MIT.txt).
[Provider mark attribution](../../clients/macos/Resources/Notices/Provider-Icons.txt)
is retained with the native app. Product names and marks belong to their respective
owners and do not imply endorsement.
