# Security Policy

Booth is a local-only desktop app: it makes no network requests, has no
accounts, and stores everything (sessions, takes, exports) on your machine
next to your script. The only OS-level capability it requests is microphone
access (`com.apple.security.device.audio-input`), used solely to record takes.

## Reporting a vulnerability

Email **security@attestrum.com**. Please include reproduction steps and the
app version (About panel or the `.dmg` filename). We aim to acknowledge
reports within a week.

Please do not open public issues for security reports until we've had a
chance to ship a fix.
