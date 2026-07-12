# Changelog

Notable changes to TokenMonitor are recorded here. Entries are newest first.

## Unreleased

- Reorganized frontend window utilities, cross-layer tests, and native test resources.
- Removed the obsolete standalone profiler, retired no-op commands and settings, and removed the unused notification integration.
- Refreshed repository documentation and ignore rules to match the current project.

## v0.14.0

- Added JSON usage import and export, including Cursor usage and synchronized SSH devices.
- Added optional automatic export to a user-selected folder.
- Improved remote-device synchronization progress, caching, and settings controls.
- Reduced repeated log scanning with refresh-aware throttling and incremental cache invalidation.
- Added payload disk caching for faster cold starts and cache warm-up controls.
- Added selectable updater channels for the official project and compatible forks.
- Improved onboarding, autostart feedback, FloatBall rate-limit controls, and Windows tray behavior.

## v0.13.7

- Included Cursor and freshly synchronized SSH data in exports.
- Throttled expensive usage-log scans to the configured refresh interval.
- Improved incremental invalidation when log files are appended.

## v0.13.6

- Added usage cache import and export.
- Improved onboarding version handling and autostart error feedback.
- Added independent provider bars for FloatBall.
- Retired the Windows taskbar panel that could freeze the system tray.

## v0.13.5

- Fixed payload disk-cache invalidation when source logs change.
- Kept the application package version in `Cargo.lock` synchronized during releases.
- Improved Windows popover positioning and resize behavior.

## v0.13.4

- Fixed updater signing-key handling in the release workflow.
- Allowed macOS updater artifacts to be signed without an Apple application certificate.

## v0.13.3

- Enabled automatic updates from compatible fork channels.
- Restored Windows test manifests and fixed cache-related regression tests.

## v0.13.2

- Repaired Cursor usage display and range-aware remote caching.
- Added a disk-backed payload cache for faster startup.
- Added a capped window-height mode for large views.

## v0.13.1 — Settings, SSH Search, and FloatBall

### Interface

- Combined Header Tabs, Models, and Remote Devices into one collapsible Visibility card with item counts.
- Consolidated status-display controls and moved their preview to the top.
- Made the FloatBall preview transparent so it matches the real overlay.
- Added a shadow below the sticky Settings header.
- Replaced SSH Add buttons with toggles; enabling an unregistered host now registers it.
- Returned Cost Alert and Model Change Stats to the Display card.

### SSH and fixes

- Expanded remote Codex discovery from `~/.codex/sessions` to matching `~/.codex*` directories.
- Added per-host connection progress and total synchronization duration.
- Fixed FloatBall utilization fallback to use any unexpired window when the primary window is stale.

## v0.12.3 — Visibility and Window Restoration

- Combined provider, model, and SSH visibility controls into one card.
- Fixed collapsed Cursor and Permissions panels leaking nested content.
- Persisted the last popover height and restored it before initial data loading.
- Prevented shrink requests until the first content render completed.
- Added a throttled callback that saves successfully applied window heights.

## v0.12.2 — Model Filtering and Chart Ordering

- Filtered cross-integration aggregate rows by model family to prevent provider cost contamination.
- Unified model-name formatting and expanded the provider color palette.
- Sorted chart legends by model cost.
- Removed unused Rust imports reported by CI.

## v0.12.1 — Signing and CI Fixes

- Fixed a Windows-only `SetWindowRgn` import path.
- Used platform-specific checksum filenames to avoid release-asset conflicts.
- Published releases as non-drafts by default.
- Applied Rust formatting and Clippy cleanups.

## v0.12.0 — Codex Limits, Transitions, and SSH Hardening

### Features

- Added Codex CLI rate-limit panels.
- Added transitions between Settings, Calendar, Devices, and single-device views.
- Added skeleton loading states.
- Required explicit local-usage permission during onboarding before parsing logs.

### Safety and correctness

- Hardened SSH alias validation and cache invalidation.
- Improved Windows timezone, SHA-256, disk I/O, and SSH streaming error paths.
- Added dismiss controls to in-app warnings.

## v0.11.1 — Cursor Limits and Updater Signing

- Added Cursor plan usage and spend-limit data through the Cursor API.
- Added Cursor credential management with OS-keyring storage and a file fallback.
- Fixed unsigned updater behavior and release-workflow secret handling on Windows and Linux.

## v0.11.0 — Backend Modules and Dynamic Rates

- Split large backend modules into focused files.
- Added exchange rates from Frankfurter with a 24-hour cache.
- Added remote-device aggregation and a dedicated Claude Code parser.
- Added persistent hourly usage archives to retain totals after source logs are deleted.
- Added dynamic OpenRouter pricing.

## v0.10.x — Permissions, Credentials, and SSH

- Added explicit permission disclosures and first-launch onboarding.
- Added Keychain-backed credential access on macOS.
- Hardened privacy disclosures and macOS TCC handling.
- Fixed SSH deduplication, removed the remote `jq` dependency, and corrected Claude session refresh behavior.

## v0.9.0 — Pie Charts and Copy Cleanup

- Added a pie-chart mode for model-share breakdowns.
- Standardized section-label capitalization.

## v0.8.0 — Auto-Updater

- Added the Rust updater state, persistence, and scheduler modules.
- Added the frontend updater store and in-app update banner.
- Added a red tray badge when an update is available.
- Added per-version notification deduplication, six-hour checks, and exponential backoff.
- Added Skip, Later, and Update actions.
- Added signed updater artifacts and a `latest.json` manifest to release builds.
- Added the [manual updater test matrix](testing/auto-update.md).

## v0.7.x — Cache Tiers, Fast Mode, and SSH Archives

- Added Claude fast mode as a separately priced model.
- Exposed cache tiers through the usage pipeline and unified their pricing.
- Added web-search tracking, SSH archive updates, and consistent rate-limit percentages.

## v0.6.0 — Cross-Platform Architecture

Compared with the upstream v0.5.0 codebase, this release introduced the foundation used by the current project:

- Split the Rust backend into domain modules for commands, rate limits, usage, statistics, logging, trays, and platforms.
- Added Windows and Linux support alongside macOS, including native tray behavior, platform window positioning, autostart, and installers.
- Added the standalone, always-on-top FloatBall overlay.
- Added SSH host discovery, remote log synchronization, per-device views, and merged remote usage.
- Added LiteLLM pricing, richer Claude parsing, and broader model coverage.
- Reorganized the Svelte frontend into components, stores, views, window helpers, tray helpers, and shared utilities.
- Added the modular installer build system and cross-platform CI/release workflows.
- Added the user tutorial and removed completed design-plan documents.
