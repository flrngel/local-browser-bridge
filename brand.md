# Brand and rebranding brief

- **Status:** Planning draft
- **Current public name:** Local Browser Bridge
- **Decision:** No replacement name, logo, or migration date has been approved.

This brief prepares a deliberate rebrand without pretending that a new brand
already exists. Until a reviewed migration is approved, product UI, packages,
documentation, release assets, and support should continue to use **Local
Browser Bridge** consistently.

## Why rebrand

The current name accurately describes the original browser bridge, but the
product now spans three related surfaces: a local control server, a Chromium
extension, and an optional native computer helper. A future brand should be
easier to remember, less implementation-shaped, and broad enough for both
browser and exact-window computer control without promising a general-purpose
remote desktop.

The rebrand should improve:

- recognition in a release list, browser toolbar, terminal, and permission
  prompt;
- comprehension for people who are not browser-extension developers;
- trust through clear local-first and human-control language;
- component naming across server, extension, helper, and dashboard; and
- room for future compatible clients without implying a bundled AI agent.

## Product truth

### One-sentence description

A local, human-revocable bridge that lets an authorized AI client use selected
browser tabs and, optionally, one selected desktop application window.

### Core promise

**Use the apps you already have open while keeping control local, visible, and
releasable.**

### What the brand must not imply

- A hosted AI agent, MCP server, or native ChatGPT, Claude, or Copilot connector
- A cloud relay, remote-access service, or cross-device tunnel
- An invisible automation daemon or separate virtual desktop
- Universal desktop authority, zero-risk automation, or guaranteed
  non-interruption
- Browser-vendor, OpenAI, Anthropic, or Microsoft ownership or endorsement
- Signed or notarized distribution before those properties actually exist

## Audience

### Primary audience

People who want an AI client on their own computer to work with an existing,
signed-in Chrome or Edge session while retaining visible stop controls.

### Secondary audiences

- Developers integrating a local agent with the authenticated dashboard or API
- Security-conscious operators evaluating authority and data boundaries
- Contributors working on browser automation, accessibility, capture, and
  release verification

### Audience needs

| Need | Brand response |
|---|---|
| “Will it work with my AI client?” | State the same-computer and client-integration boundary before features |
| “What must I install?” | Present server + extension first and the helper as optional |
| “Can it take over everything?” | Describe exact tab/window scope, Safe mode, visible control, and release paths |
| “Is it sending my data to a vendor?” | Say loopback-only and no project cloud relay; avoid absolute privacy claims about third-party AI clients |
| “Can I trust the download?” | Lead to immutable releases, checksums, provenance, and current signing status |

## Positioning

### Positioning statement

For people and developers whose AI client can operate locally, the product is a
loopback control bridge for existing browser tabs and one optional app window.
Unlike a cloud browser or remote desktop, it keeps the product's transport on
the same computer and exposes explicit, human-owned control boundaries.

### Differentiators

- Existing signed-in browser session instead of a separate hosted browser
- Browser-owned debugging indication plus a controlled-page Stop surface
- Optional native exact-window helper rather than mandatory whole-desktop
  control
- Safe mode and Full Access as explicit authority choices
- Version-matched, independently verifiable release assets
- Negative evidence and limitations retained instead of hidden

## Brand principles

1. **Local is concrete.** Say what binds to loopback and what still depends on a
   third-party AI client. Do not use “private” as a shortcut.
2. **Power stays visible.** Control state, scope, and release actions should be
   understandable without reading protocol documentation.
3. **Scope beats magic.** Prefer “selected tab” and “one selected app window”
   over “control your computer.”
4. **Evidence beats superlatives.** Use “verified,” “supported,” or “proven” only
   with a named release, platform, and evidence boundary.
5. **Plain language leads.** Introduce dashboard, extension, helper, lease, and
   token only when each term becomes useful.
6. **Failures remain part of the story.** A failed-closed result is a trust
   feature, not marketing material to erase.

## Personality

The brand should feel:

- capable, not theatrical;
- calm, not passive;
- transparent, not alarming;
- technical, not cryptic; and
- independent, not anti-vendor.

Avoid the common “AI magic” personality: no sentient-assistant language,
autonomy theater, robot mascots, glowing brains, or promises that the user can
“set it and forget it.”

## Voice and writing

| Prefer | Avoid |
|---|---|
| “Select one app window.” | “Give the AI your computer.” |
| “The server listens on `127.0.0.1`.” | “Everything is completely private.” |
| “Release control at any time.” | “The agent is always under control.” |
| “This release passed the named acceptance gate.” | “SOTA,” “perfect,” or “unbreakable.” |
| “The client must run on the same computer.” | “Works with every AI assistant.” |
| “Windows binaries are not yet publisher-signed.” | Hiding a platform warning in troubleshooting |

Writing rules:

- Lead with the user outcome, then state the boundary.
- Use **server**, **extension**, **computer helper**, and **dashboard**
  consistently.
- Use **control lease** only after explaining that it is a time-bounded period
  of active control.
- Use **release control** for the user action and **revoke** for protocol or
  security behavior.
- Name Chrome and Edge only as supported products, never as part of the future
  master brand.
- Keep all repository, UI, release, log, and support text in English.

## Naming brief

### Requirements for a new master name

A candidate name should:

- be short enough for an extension card, toolbar popup, app title, and terminal
  banner;
- be pronounceable and spellable after hearing it once;
- work for browser and exact-window control without implying a remote desktop;
- suggest locality, linkage, scope, visibility, or handoff without requiring a
  technical explanation;
- have a usable repository slug and command-line abbreviation;
- avoid “AI” as the sole differentiator; and
- survive basic trademark, company-name, package-registry, domain, GitHub,
  browser-store, and search-result checks.

Do not approve a name from availability intuition. Record the search date,
jurisdictions, registries, conflicts, and decision owner. A legal professional
must evaluate trademark risk before commercial adoption.

### Naming territories to explore

These are directions, not candidate names:

1. **Local link** — emphasizes same-computer connection and interoperability.
2. **Visible handoff** — emphasizes deliberate transfer and return of control.
3. **Scoped window** — emphasizes exact targets and bounded authority.
4. **Beacon or status** — emphasizes visible activity and human awareness.

Avoid names centered on bridges, bots, copilots, browsers, Chrome, remote
desktops, omnipotence, stealth, or background autonomy unless research shows a
clear, defensible reason.

### Name scorecard

Score each shortlisted name from 1 to 5 and retain the notes, not just the
total.

| Criterion | Weight | Question |
|---|---:|---|
| Product fit | 5 | Does it cover both browser and optional exact-window control? |
| Trust | 5 | Does it sound user-controlled rather than invasive or hidden? |
| Distinctiveness | 4 | Is it memorable in the software and AI tooling market? |
| Comprehension | 4 | Can a new user form a roughly correct expectation? |
| Availability | 5 | Did recorded legal, registry, domain, and search checks find a viable path? |
| Cross-platform use | 3 | Does it work in Windows, macOS, browser, terminal, and release contexts? |
| Accessibility | 3 | Is it easy to pronounce, dictate, spell, and distinguish? |
| Migration cost | 2 | Can public names change while compatibility aliases remain clear? |

## Product naming architecture

The final naming system needs these roles:

| Role | Current label | Future pattern |
|---|---|---|
| Master product | Local Browser Bridge | `[Master brand]` |
| Local server | Local Browser Bridge server | `[Master brand] Server` or the unmodified master name |
| Browser component | Local Browser Bridge extension | `[Master brand] Extension` |
| Desktop component | Local Computer Helper | `[Master brand] Computer Helper` |
| Local web UI | authenticated control page/dashboard | `[Master brand] Dashboard` |
| Restricted authority | Safe mode | Keep a plain functional label unless research supports a clearer term |
| Broad authority | Full Access | Keep an explicit authority label; never soften it into marketing copy |

Component names must reveal what the component does. Do not give every binary a
different metaphorical sub-brand.

## Current visual baseline

The repository currently has one reusable product-identity asset:
`public/favicon.svg`. It uses a dark rounded square, opposing lime brackets, and
a cyan center node. The dashboard follows the same near-black, lime, cyan,
amber-warning, and coral-danger system with system and monospace type.

| Current role | Current value |
|---|---|
| Page background | `#0A0C0B` |
| Panel background | `#121614` |
| Primary accent | `#B8F55D` |
| Secondary accent | `#72E7DB` |
| Warning | `#FFD479` |
| Danger | `#FF8B72` |

There is no complete logo system, extension icon set, Windows `.ico`, macOS
`.icns`, wordmark, monochrome mark, light-mode mark, social image, or store
artwork. The existing favicon and dashboard palette are inputs to research, not
approved permanent brand assets.

## Visual direction

### Design idea

Explore a simple mark built from **two bounded surfaces plus one deliberate
connection**. It should communicate scoped handoff rather than a generic network
bridge. The negative space can suggest a browser tab or app window without
copying a vendor icon.

### Avoid

- Chrome, Edge, Windows, macOS, OpenAI, Anthropic, or Microsoft logo geometry
- Generic sparkles, robot heads, brains, magic wands, infinity loops, and cloud
  outlines
- Padlocks as the main symbol; the product is powerful software, not a security
  certification
- Mouse-pointer imagery that implies every action uses the shared physical
  pointer
- Gradients or fine detail that fail at a 16-pixel extension icon

### Asset requirements

The chosen system must include:

- primary horizontal and compact lockups;
- an icon that remains recognizable at 16, 32, 48, and 128 pixels;
- SVG masters plus deterministic PNG exports;
- light, dark, monochrome, and high-contrast variants;
- a browser-toolbar state set for disconnected, connected, active, paused, and
  error states;
- a macOS app icon and Windows executable icon derived from the same geometry;
- a social/repository preview image; and
- documented safe area, minimum size, and incorrect-use examples.

Color cannot be the only status signal. Text, shape, or icon changes must
distinguish connected, active, paused, warning, and error states. All essential
text and controls should meet WCAG 2.2 AA contrast requirements.

### Color and type research

Do not select a final palette or typeface in this brief. Test candidates against
these requirements:

- clear separation between neutral connection state and active authority;
- warning and destructive colors that remain recognizable with common color
  vision deficiencies;
- system-font fallback for the dashboard and extension popup;
- readable terminal output without relying on color; and
- licenses that permit repository, binary, web, and marketing distribution.

## Trust UX is part of the brand

The rebrand must preserve or improve these non-negotiable behaviors:

- Active browser control remains visible in both browser chrome and the target
  page when the platform exposes those surfaces.
- **Stop**, **Cancel**, **Release control**, and token clearing use direct,
  consistent language.
- The optional helper stays optional and names the exact selected window.
- Safe mode and Full Access remain distinguishable before control starts.
- Permission requests explain why Screen Recording or Accessibility is needed
  and which component uses it.
- Signing, notarization, update, and release-verification status is stated at
  the decision point.
- No rename removes negative evidence, known limitations, or the independent
  project disclaimer.

## Rename inventory and compatibility plan

A rebrand is not a global search-and-replace. The following surfaces have
different migration risks.

| Surface | Current value or pattern | Category | Migration rule |
|---|---|---|---|
| README, website copy, screenshots | Local Browser Bridge | Public brand | Change together at public launch |
| Extension display name and title | Local Browser Bridge | Public brand | Change with new icon/copy and a live extension acceptance run |
| Dashboard and controlled-page copy | Browser Bridge / Local Browser Bridge | Public brand and trust UI | Make consistent atomically; preserve Stop/release semantics and reviewed selectors |
| macOS app display name | Local Computer Helper | Public component label | Change with the app bundle, permission copy, docs, and package verification |
| Windows VERSIONINFO | Local Browser Bridge / contributors | Public binary metadata | Change with artifact verifier and signing plan |
| Windows assembly identity | `dev.flrngel.LocalBrowserBridge` | Embedded OS application identity | Change only with the application manifest, artifact verifier, compatibility review, and clean-host testing |
| GitHub repository | `flrngel/local-browser-bridge` | Distribution identity | Rename only after redirects, workflow permissions, release URLs, and documentation are tested |
| Cargo package and binary | `local-browser-bridge` | Developer and automation interface | Keep a compatibility binary or documented transition window |
| Helper binary | `local-computer-helper` | Developer and automation interface | Keep a compatibility binary or documented transition window |
| Release asset names | `local-browser-bridge-*`, `local-computer-helper-*` | Immutable distribution contract | Never rename historical assets; version and verify a new schema for future assets |
| Environment variables | `LBB_PORT`, `LBB_TOKEN`, `LBB_TOKEN_PATH`, `LBB_DISABLE_UPDATE_CHECK` | Configuration API | Add new aliases first; reject conflicting dual values; deprecate only after a stated support window |
| Token directory | `.local-browser-bridge` | Persistent user data | Read and migrate safely without regenerating or exposing the credential |
| Protocol origin and domain | `lbb-computer-helper://local`, `LBB-WS-AUTH-V1` | Authentication compatibility and security boundary | Do not silently rename; require an explicit protocol version and cross-version tests |
| macOS bundle ID and signing identity | `dev.flrngel.local-browser-bridge.computer-helper` plus the release signature | OS identity and permission continuity | Preserve by default; any change is a permission and signing migration, and current ad-hoc signing means continuity must be retested even when the bundle ID is retained |
| Acceptance bundle IDs | `dev.flrngel.local-browser-bridge.acceptance.*` | Test evidence contract | Version fixtures, schemas, and verifiers together |
| Extension messages and globals | `LBB_*`, `__LBB_*`, and current accessible labels | Internal and test compatibility | Inventory every consumer before changing; retain compatibility only when it does not weaken trust checks |
| Updater and release URLs | fixed `flrngel/local-browser-bridge` API and asset paths | Trust and update contract | Update repository, workflow, provenance, verifier, and fallback behavior together |
| Evidence and historical release text | versioned Local Browser Bridge records | Immutable history | Never rewrite; add a note mapping the former and new names |

Before implementation, generate a complete machine-readable rename inventory
from tracked files and classify every match as public copy, stable interface,
security domain, OS identity, test contract, immutable history, or third-party
attribution.

## Rollout plan

### Phase 0: discovery

- Interview at least five target users about product comprehension and trust.
- Produce 10 to 20 name candidates across the approved naming territories.
- Run recorded legal, registry, domain, repository, package, extension-store,
  and search checks.
- Audit every current name and `LBB` identifier in source, artifacts, scripts,
  docs, fixtures, and evidence.

### Phase 1: decision and design

- Select the master name and component architecture with a named decision owner.
- Create the logo, icon state set, color tokens, type rules, and copy deck.
- Test 16-pixel recognition, dark, light, and high-contrast modes, color-blind
  status differentiation, pronunciation, and first-impression comprehension.
- Write a migration RFC covering compatibility, credentials, permissions,
  release schemas, rollback, and support duration.

### Phase 2: compatibility release

- Add new display names and safe aliases without removing old command,
  environment, token, or protocol paths.
- Detect conflicting old and new configuration explicitly.
- Add tests for upgrade, downgrade refusal where necessary, clean install,
  uninstall, and rollback.
- Publish migration documentation before asking users to update.

### Phase 3: public launch

- Update README, UI copy, extension metadata, binary metadata, screenshots,
  repository description, release notes, and support templates together.
- Publish one verified release containing the complete public rename.
- Test Windows install and update, macOS permission behavior, extension loading,
  browser control, desktop helper control, update checks, and revocation on the
  exact public assets.
- Keep a “formerly Local Browser Bridge” bridge for search and support.

### Phase 4: measured cleanup

- Remove compatibility aliases only after the documented support window and
  telemetry-independent evidence such as issue and support review.
- Never mutate historical tags, releases, attestations, evidence, or checksums.
- Retain a permanent former-name note in security and release documentation.

## Required deliverables before implementation

- Approved name brief and scored shortlist
- Availability and trademark-risk record
- Final naming architecture
- Logo and icon source package with export script
- Color, typography, spacing, and status-state tokens
- Voice, terminology, and UI copy deck
- Machine-readable rename inventory
- Compatibility and credential-migration RFC
- Windows, macOS, extension, update, and rollback test plan
- Launch checklist and post-launch support plan

## Rebrand acceptance criteria

The rebrand is ready to ship only when:

- a new user can explain what the product controls and what it does not control
  after reading the first screen;
- the selected name has a recorded availability and risk decision;
- status is understandable without color and assets work at required sizes;
- the server, extension, helper, dashboard, docs, and release page use one
  coherent naming architecture;
- old credentials and supported configuration paths migrate without exposure or
  silent reset;
- protocol, package, update, and release verifiers understand the transition;
- Windows and macOS clean installs and upgrades pass on exact release assets;
- browser and desktop Stop or release behavior is unchanged or stronger; and
- historical releases and evidence remain byte-for-byte immutable.

## Open decisions

| Decision | Owner | Due | Status |
|---|---|---|---|
| Master name | TBD | TBD | Open |
| Component naming pattern | TBD | TBD | Open |
| Trademark jurisdictions and reviewer | TBD | TBD | Open |
| Repository rename timing | TBD | TBD | Open |
| Compatibility support window | TBD | TBD | Open |
| Code-signing and notarization target | TBD | TBD | Open |
| Public launch release | TBD | TBD | Open |

Do not start the production rename until these decisions have named owners and
the migration RFC is approved.
