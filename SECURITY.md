# Security policy

Trindle screen reader is a small project kept up in the time available, not a team with a dedicated security function. This page sets realistic expectations.

## Supported versions

Only the latest release is supported. There is no long-term-support branch and no backporting. A fix lands in the next release and you are expected to update.

| Version | Supported |
|---|---|
| 0.1.x | yes |

## Reporting a vulnerability

**Please do not put the details of a security problem in a public issue.** A public report tells everyone, including people running an unpatched copy, before a fix exists.

Write to <distro@trindle.org> with "Trindle screen reader security" in the subject line and say only that you have a security issue. Leave the details out of that first message and a private channel will be arranged. GitHub's private vulnerability reporting, under **Security** then **Report a vulnerability**, also reaches only the maintainers.

Every other kind of bug, crash or idea goes to the [Buildmorphic community](https://buildmorphic.com) first; GitHub issues are also welcome. Please do not post a security problem there.

## What to expect

Replies and fixes are best effort, not guaranteed against any service level. You will be told when the report is read, and when a fix is released.

## What is in scope

Anything that makes the reader speak a password or a secret field, talk to the network, accept keys from a program that is not the compositor, or from another user, or crash or hang on what another program tells it. A fault in a program the reader is reading is not in scope.
