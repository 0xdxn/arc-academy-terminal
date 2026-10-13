# Security

arct runs commands on people's computers, and some of those people are children, so safety bugs matter here more than most.

## Reporting

Please report privately: use **Report a vulnerability** on the repository's Security tab (GitHub private vulnerability reporting). Don't open a public issue for:

- a way for a command to escape `~/ArcAcademy/home`, the lesson playground, or the simulated sandbox;
- a destructive command the guard should refuse but doesn't;
- the installer accepting a file that doesn't match the published checksums;
- anything that sends data off the learner's computer without them asking.

Include what you ran and what happened. We'll confirm we got it, fix it, and credit you if you'd like.

## What arct promises

- No accounts, analytics, or telemetry sent anywhere. Optional usage stats stay in a local file and are off by default.
- The AI helper is off by default. When on, questions go only to the provider the user picked.
- Safe mode (the default) keeps the free shell inside `~/ArcAcademy/home`. With it off, catastrophic commands are still refused.
- The installer verifies SHA-256 checksums before installing anything.
