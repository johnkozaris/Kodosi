# Security direction

Kodosi's product requirement is end-to-end encryption of terminal traffic and room
content. Participants have full control of shared terminals; restricted roles and
sandboxing are not part of this model. See [PRODUCT.md](../PRODUCT.md) for room sharing
and the capabilities still to build.

## Current terminal implementation

- A new device shows a code. The user types it on an approved device. The service
  never gets the code, so it cannot add a device to an account.
- Each account keeps a list of friend identities that its own devices sign. A device
  uses a friend's identity only when it is equal to the recorded one, and shows a
  changed identity until the user trusts it again. An invite text lets two people
  verify each other without an extra step.
- Each view has its own TLS 1.3 channel between the viewer device and the host device,
  with a new classic and post-quantum key exchange and with both device keys proved.
  The service copies the encrypted records and holds no terminal key.
- The host admits a viewer only when the account is its own or one that it shared the
  terminal with, and the device key is in the verified device list of that account.
- A sharing change or a device removal closes the affected views at once and does not
  disturb other viewers.

The reasons, the measured results and the full list of risks are in
[PROTOCOL-16.md](PROTOCOL-16.md).

## Limits

- A friend added by username and not verified is taken from the service one time, when
  the friendship starts. The mark "not verified" shows this.
- The service reads terminal names, host names, device labels and handles, and sees
  who connects to whom, when, and padded sizes.
- A stolen device has the access of that device until an owner removes it.

Room conversation and room-based terminal sharing are not implemented yet. Their
encryption must follow the product's membership and sharing behavior. The current
per-person terminal checks above describe today's code, not a separate product rule.
