# Contributing to Kodosi

Kodosi brings people and their coding agents together through shared terminals,
conversation, and tasks. Read [PRODUCT.md](PRODUCT.md) for product intent and
[development setup](docs/DEVELOPMENT.md) to build it.

## Start with the problem

Use [GitHub issues](https://github.com/johnkozaris/Kodosi/issues) for bugs and feature
proposals. For a bug, include the platform, source revision, what you did, what you
expected, and what happened. Redact private terminal content and credentials from
logs and screenshots.

For a larger change, describe the user problem and proposed behavior in an issue
before adding a new subsystem. Small fixes can go straight to a pull request.
Report vulnerabilities [privately](docs/SECURITY.md#report-a-vulnerability).

## Make a focused change

Keep dependencies project-local and follow the nearest `AGENTS.md` or `CLAUDE.md`.
The runtime owns terminal behavior; native clients present it; the backend stores
shared metadata and routes encrypted traffic. There is one repository for all of it.

Full control of shared terminals is intentional. Give people and agents useful
capabilities without adding permission tiers, a second agent harness, or a required
shared checkout.

Keep generated contracts and native pins aligned. Preserve license notices and
required corresponding source. Update the relevant user guide when behavior changes;
keep setup commands in the development and platform guides.

## Verify the result

Run the checks appropriate to the change from the root `justfile`. Validate changed
workflows by running the app or calling its real protocol with isolated test data.
For UI changes, include a screenshot or recording of the result in the pull request.
Add focused regression tests where they protect meaningful behavior; do not add
automated smoke drivers.

Never reset a live database or modify real provider credentials, conversations,
memory, settings, or working files for a test. See
[isolated validation](docs/DEVELOPMENT.md#isolated-validation).

## Open the pull request

Explain the problem, the resulting behavior, and how you verified it. Call out any
remaining limitation. Keep unrelated changes separate so a reviewer can assess the
whole change.

Real credentials, signing keys, local sessions, and personal configuration stay
outside Git. Use visibly dummy examples; generate credential-shaped test data at
runtime when possible. Public API addresses, the OAuth public-client ID, and the
Apple signing team identifier are intentional public configuration.

By contributing, you agree that your contribution is available under Kodosi's
[MIT license](LICENSE).
