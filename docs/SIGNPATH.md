# Applying to SignPath Foundation (free code signing)

**Status: prepared, not sent.** Nothing here has been submitted. This is a checklist for when the maintainer decides to apply.

A code-signing certificate would make the "Windows protected your PC" (SmartScreen) prompt go away over time. [SignPath Foundation](https://signpath.org) gives one for free to open-source projects that meet its [terms](https://signpath.org/terms). The certificate's publisher would be **"SignPath Foundation"**, not "Fishy Client".

Read on 2026-11-25. The terms page is marked "Draft", so check it again before applying. The apply page ([signpath.org/apply](https://signpath.org/apply)) didn't show its form when read automatically: open it in a browser.

## The blocker: releases are built on the maintainer's PC

SignPath only signs files that an automated CI build produced from the public repository, so it can prove where each file came from (on GitHub: GitHub Actions plus SignPath's signing action). **Fishy Client's releases are built and signed on the maintainer's PC** ([RELEASING.md](RELEASING.md)), and **moving the release build to GitHub Actions is postponed by the maintainer's choice.** Until that move happens, Fishy Client can't use SignPath.

The CI workflow (`.github/workflows/ci.yml`) already builds and tests every push on `windows-latest`, which is most of the way there. It doesn't make release files yet.

## Checklist

| Requirement (from the terms) | Fishy Client today |
|---|---|
| OSI-approved licence, no commercial dual-licensing | ✅ GPL-3.0-or-later |
| No proprietary components | ✅ All code is open source. No Epic or game files are bundled. Check again before applying. |
| No malware or unwanted programs; no hacking tools; nothing that gets around the PC's security | ✅ It's a game launcher. ⚠️ See "Things to ask SignPath" below. |
| Actively maintained, and already released in the form to be signed | ✅ Released as an MSI on GitHub since 1.0.0 |
| What it does is described on the download page | ✅ README and each Release page |
| **Built in CI from the public repository, verifiably** | ❌ **Postponed**: releases are built on the maintainer's PC (see above) |
| **Manual approval of every release** | ❌ To set up in SignPath, once releases are built in CI |
| **MFA for every team member** on GitHub and SignPath | ☐ Turn on two-factor authentication for the GitHub account (Settings → Password and authentication), and use it on SignPath too |
| Team roles listed: Authors, Reviewers, Approvers | ☐ Add a "Code signing policy" section (draft below) |
| Every change from outside the team is reviewed | ✅ Today one maintainer reviews every pull request |
| Privacy policy linked, covering third-party services | ✅ README → [Privacy](../README.md#privacy) |
| Announce system changes; provide uninstall | ✅ Start with Windows and shortcuts are opt-in. Using Epic's folder asks for admin rights once per game. Uninstall works from Windows Settings, and Delete all my data removes the rest |
| Product name and version the same in every file's metadata | ☐ Check in the MSI and `fishy-client.exe` properties when release builds move to CI |
| "Code signing policy" named on the home page and the download/release page | ☐ Add the README section below, and a link to it in each Release's notes |

## Draft README section (add only once accepted)

```markdown
## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org)

- Committers and reviewers: [Gnammahh](https://github.com/Gnammahh)
- Approvers: [Gnammahh](https://github.com/Gnammahh)

Every release is built by GitHub Actions from this repository and signed only after a maintainer approves it.

Privacy: Fishy Client talks only to Epic (to sign in, list, download and start your games) and to GitHub (to check for its own updates). It sends no telemetry. See [Privacy](#privacy).
```

The attribution line must stay word for word as above.

## Things to ask SignPath before applying

- **The launcher presents itself as Epic's launcher to Epic's servers** (the same client ID and user agent Legendary and Heroic use), and it's unofficial. The terms say nothing about trademarks or impersonation, but this is the part most likely to be questioned. Ask support@signpath.io whether a third-party launcher like this is acceptable, pointing to Legendary and Heroic as similar open-source projects.
- Whether the publisher name "SignPath Foundation" on the installer is fine with how the README describes the project.

## Steps, when the time comes

1. Move the release build to GitHub Actions (a separate decision). Keep the updater key out of the repository, as a GitHub encrypted secret.
2. Turn on MFA on GitHub.
3. Fill in the form at [signpath.org/apply](https://signpath.org/apply).
4. Once accepted: add SignPath's GitHub action to the release workflow, set manual approval, add the README section above, and link "Code signing policy" from each Release.
