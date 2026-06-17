#!/usr/bin/env node
// ─────────────────────────────────────────────────────────────────────────────
// TEMPORARY cleanup script — merge + dedup TokenMonitor auto-sync JSONL files.
//
// Reads every `TokenMonitor-Usage-*.jsonl` in an input folder (the cloud-sync
// folder), merges them, and writes ONE deduped JSONL file. NON-DESTRUCTIVE: it
// never deletes or overwrites the input files; it writes a new output file.
//
// Dedup, two passes:
//   1. Exact-line dedup — identical record lines collapse to one.
//   2. Cross-device dedup — record lines whose (provider + record usage fields)
//      are identical but whose `device`/`source_key` differ are treated as ONE
//      datum (the same usage attributed to several devices, i.e. the duplicate
//      bug). For each such group we keep exactly ONE line:
//        • Drop "local-ish" lines first: source_key `local:*` OR a `device`
//          label that ends in "(macOS)"/"(Windows)"/"(Linux)" (a machine's own
//          local view, incl. drifted self-duplicate device entries).
//        • Among the remaining (SSH-host / remote) lines, keep the one with the
//          lexicographically smallest `source_key` (tiebreak: smallest device).
//        • If a group has ONLY local-ish lines, keep the lexicographically
//          smallest one (never drop the datum entirely).
//
// The "record usage fields" used for matching are the semantic ones
// (provider,d,h,mk,in,out,c5,c1,cr,ws); the derived `cost`/`mn` are ignored so
// pricing drift between exports doesn't defeat the dedup.
//
// NON-LOCAL DEVICES EXTRACTION (second output):
//   The script ALSO writes a separate importable file containing every
//   NON-LOCAL device, so importing it reproduces the app's full device list:
//     • Real SSH hosts: rows whose `source_key` is already `device:<alias>`,
//       kept verbatim.
//     • Peer MACHINES: in a raw auto-sync file every machine's OWN data is
//       `local:*`. The app's auto-sync turns a peer's file into a device by
//       remapping its `local:*` → `device:<filename-slug>` (the slug is the
//       part of the file name between the prefix and suffix). This script does
//       the SAME, so each peer laptop/desktop shows up as its own device —
//       instead of only the SSH hosts.
//   The machine you import INTO is the "self" machine: its own `local:*` stays
//   local (NOT turned into a device) so it isn't double-counted. Name it with
//   `--self=<slug>`; if omitted, the most-recently-written input file is
//   assumed to be this machine (auto-sync keeps rewriting its own file). Pass
//   `--self=none` to turn EVERY machine into a device (use when importing into
//   a fresh machine that has no local data of its own).
//   The output header carries no device, so import treats it as a verbatim
//   restore of the already-correct `device:*` source keys (never touches
//   local). A per-device breakdown is printed; `--hosts=a,b` narrows the file
//   to specific device aliases.
//
// Usage:
//   node scripts/tmp-merge-dedup-usage.mjs <inputDir> [outputFile] [flags]
//   node scripts/tmp-merge-dedup-usage.mjs ~/Dropbox/TokenMonitor
//   node scripts/tmp-merge-dedup-usage.mjs ~/Dropbox/TokenMonitor --self=MyMac-macOS-ab12cd34
// Flags:
//   --hosts=a,b,c     only include these device aliases in the devices file
//   --ssh-out=PATH    path for the devices file (default <inputDir>/…ssh-hosts.jsonl)
//   --self=<slug>     the import-target machine; its local: stays local
//                     (default: newest input file; "none" = every machine is a device)
// If outputFile is omitted, writes <inputDir>/TokenMonitor-Usage-merged.jsonl
// and <inputDir>/TokenMonitor-Usage-ssh-hosts.jsonl.
// ─────────────────────────────────────────────────────────────────────────────

import { readdirSync, readFileSync, writeFileSync, statSync } from "node:fs";
import { join, basename } from "node:path";

const FILE_PREFIX = "TokenMonitor-Usage-";
const FILE_SUFFIX = ".jsonl";
const JSONL_FORMAT = "tokenmonitor-usage-export-jsonl";
const OS_LABEL_RE = /\((?:macOS|Windows|Linux)\)\s*$/i;

function fail(msg) {
  console.error(`error: ${msg}`);
  process.exit(1);
}

const inputDir = process.argv[2];
if (!inputDir) {
  fail(
    "missing <inputDir>. Usage: node scripts/tmp-merge-dedup-usage.mjs <inputDir> [outputFile]",
  );
}
try {
  if (!statSync(inputDir).isDirectory()) fail(`${inputDir} is not a directory`);
} catch {
  fail(`cannot read directory ${inputDir}`);
}

// ── Parse remaining args: one positional outputFile + optional flags ──
let outputFile;
let sshOutputFile;
let hostAllowList = null; // null = include every device:* host
let selfArg; // undefined = auto (newest file); "none" = no self machine
for (const a of process.argv.slice(3)) {
  if (a.startsWith("--hosts=")) {
    hostAllowList = new Set(
      a
        .slice("--hosts=".length)
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean),
    );
  } else if (a.startsWith("--ssh-out=")) {
    sshOutputFile = a.slice("--ssh-out=".length);
  } else if (a.startsWith("--self=")) {
    selfArg = a.slice("--self=".length).trim();
  } else if (!a.startsWith("--") && !outputFile) {
    outputFile = a;
  }
}
outputFile ||= join(inputDir, `${FILE_PREFIX}merged${FILE_SUFFIX}`);
sshOutputFile ||= join(inputDir, `${FILE_PREFIX}ssh-hosts${FILE_SUFFIX}`);

// ── Collect input files (skip our own outputs if they already exist) ──
const skipNames = new Set([basename(outputFile), basename(sshOutputFile)]);
const files = readdirSync(inputDir)
  .filter(
    (f) =>
      f.startsWith(FILE_PREFIX) && f.endsWith(FILE_SUFFIX) && !skipNames.has(f),
  )
  .sort();

if (files.length === 0) {
  fail(`no ${FILE_PREFIX}*${FILE_SUFFIX} files found in ${inputDir}`);
}

// The device alias the app derives from an auto-sync file name (= the slug
// between the prefix and suffix). A peer machine's local:* is remapped to
// device:<this slug>.
const fileSlugOf = (name) =>
  name.slice(FILE_PREFIX.length, name.length - FILE_SUFFIX.length);

// Resolve the "self" machine (its local: stays local, never a device). Default
// to the newest input file — auto-sync continuously rewrites THIS machine's own
// export, so it is reliably the most recently modified.
let selfSlug;
if (selfArg === "none") {
  selfSlug = null;
} else if (selfArg) {
  selfSlug = selfArg;
} else {
  let newest = null;
  let newestMtime = -Infinity;
  for (const f of files) {
    const m = statSync(join(inputDir, f)).mtimeMs;
    if (m > newestMtime) {
      newestMtime = m;
      newest = f;
    }
  }
  selfSlug = newest ? fileSlugOf(newest) : null;
}

// ── Read + parse every record line ──
/** @type {{source_key:string, device:string, provider:string, record:object, raw:string}[]} */
const rows = [];
let headerLines = 0;
let malformed = 0;

for (const f of files) {
  const fileSlug = fileSlugOf(f);
  const text = readFileSync(join(inputDir, f), "utf8").replace(/^﻿/, "");
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line) continue;
    let obj;
    try {
      obj = JSON.parse(line);
    } catch {
      malformed++;
      continue;
    }
    // Header line: has `format`, no source_key/record.
    if (obj && obj.format === JSONL_FORMAT && obj.source_key === undefined) {
      headerLines++;
      continue;
    }
    if (!obj || typeof obj.source_key !== "string" || typeof obj.record !== "object") {
      malformed++;
      continue;
    }
    rows.push({
      source_key: obj.source_key,
      device: typeof obj.device === "string" ? obj.device : "",
      provider: typeof obj.provider === "string" ? obj.provider : "",
      record: obj.record,
      fileSlug,
      // Canonical re-serialization for exact-line dedup (stable key order).
      raw: stableStringify({
        source_key: obj.source_key,
        device: obj.device,
        provider: obj.provider,
        record: obj.record,
      }),
    });
  }
}

// Stable stringify so key ordering can't create false "distinct" lines.
function stableStringify(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(",")}]`;
  const keys = Object.keys(value).sort();
  return `{${keys.map((k) => `${JSON.stringify(k)}:${stableStringify(value[k])}`).join(",")}}`;
}

// ── Pass 1: exact-line dedup ──
const exactSeen = new Set();
const afterExact = [];
for (const row of rows) {
  if (exactSeen.has(row.raw)) continue;
  exactSeen.add(row.raw);
  afterExact.push(row);
}
const exactDropped = rows.length - afterExact.length;

// ── Pass 2: cross-device dedup on (provider + usage fields) ──
function usageKey(row) {
  const r = row.record || {};
  // Semantic identity only (ignore derived cost / display mn).
  return [
    row.provider,
    r.d,
    r.h,
    r.mk,
    r.in,
    r.out,
    r.c5,
    r.c1,
    r.cr,
    r.ws,
  ].join("|");
}

function isLocalish(row) {
  if (row.source_key.startsWith("local:")) return true;
  if (OS_LABEL_RE.test(row.device)) return true; // own machine label incl. drifted self-dup devices
  return false;
}

/** @type {Map<string, typeof afterExact>} */
const groups = new Map();
for (const row of afterExact) {
  const k = usageKey(row);
  const g = groups.get(k);
  if (g) g.push(row);
  else groups.set(k, [row]);
}

const kept = [];
let crossDropped = 0;
for (const g of groups.values()) {
  if (g.length === 1) {
    kept.push(g[0]);
    continue;
  }
  const nonLocal = g.filter((r) => !isLocalish(r));
  const pool = nonLocal.length > 0 ? nonLocal : g;
  // Smallest source_key, then smallest device, for a deterministic winner.
  pool.sort(
    (a, b) =>
      a.source_key.localeCompare(b.source_key) ||
      a.device.localeCompare(b.device),
  );
  kept.push(pool[0]);
  crossDropped += g.length - 1;
}

// Deterministic output order.
kept.sort(
  (a, b) =>
    a.source_key.localeCompare(b.source_key) ||
    usageKey(a).localeCompare(usageKey(b)),
);

// ── Build the NON-LOCAL devices rows ──
// Mirrors the app's auto-sync peer merge: a `device:<alias>` row (a real SSH
// host) passes through verbatim; a PEER machine's `local:*` row becomes
// `device:<its-file-slug>` so the machine shows up as its own device. The self
// machine's `local:*` is dropped here — it stays local on import.
function deviceAliasOf(row) {
  if (row.source_key.startsWith("device:")) {
    return row.source_key.slice("device:".length);
  }
  if (row.source_key.startsWith("local:")) {
    if (selfSlug !== null && row.fileSlug === selfSlug) return null; // self stays local
    return row.fileSlug; // peer machine → its own device
  }
  return null;
}
const allSshRows = [];
for (const row of kept) {
  const alias = deviceAliasOf(row);
  if (alias === null) continue;
  const source_key = `device:${alias}`;
  allSshRows.push(source_key === row.source_key ? row : { ...row, source_key });
}
const sshRows = hostAllowList
  ? allSshRows.filter((r) =>
      hostAllowList.has(r.source_key.slice("device:".length)),
    )
  : allSshRows;

// Per-device breakdown (over ALL device rows, so you can see what's available
// even when an allow-list narrows the written file).
const aliasCounts = new Map();
for (const r of allSshRows) {
  const a = r.source_key.slice("device:".length);
  aliasCounts.set(a, (aliasCounts.get(a) || 0) + 1);
}

// ── Write output ──
// Header carries NO device/device_id on purpose: TokenMonitor's import then
// treats this as a verbatim restore (the source_keys are already correct), not
// as a peer file to be re-remapped.
function writeImportFile(path, outRows) {
  const header = {
    format: JSONL_FORMAT,
    format_version: 1,
    exported_at: new Date().toISOString(),
    app_version: "merged-by-tmp-script",
    device: "",
  };
  const lines = [JSON.stringify(header)];
  for (const row of outRows) {
    lines.push(
      JSON.stringify({
        source_key: row.source_key,
        device: row.device,
        provider: row.provider,
        record: row.record,
      }),
    );
  }
  writeFileSync(path, lines.join("\n") + "\n", "utf8");
}
writeImportFile(outputFile, kept);
writeImportFile(sshOutputFile, sshRows);

// ── Report ──
console.log(`Files merged:           ${files.length}`);
console.log(`Header lines skipped:   ${headerLines}`);
console.log(`Malformed lines:        ${malformed}`);
console.log(`Record lines read:      ${rows.length}`);
console.log(`Exact duplicates:       -${exactDropped}`);
console.log(`Cross-device duplicates:-${crossDropped}`);
console.log(`Lines written (merged): ${kept.length}`);
console.log(`Merged output:          ${outputFile}`);
console.log("");
console.log(`Self machine (kept local): ${selfSlug ?? "(none — all are devices)"}`);
const peerMachineSlugs = new Set(files.map(fileSlugOf));
console.log(`Devices found:          ${aliasCounts.size}`);
for (const [alias, count] of [...aliasCounts.entries()].sort()) {
  const skipped = hostAllowList && !hostAllowList.has(alias) ? " (skipped)" : "";
  const kind = peerMachineSlugs.has(alias) ? " [peer machine]" : "";
  console.log(`  device:${alias}  ->  ${count} line(s)${kind}${skipped}`);
}
console.log(`Device lines written:   ${sshRows.length}`);
console.log(`Devices output:         ${sshOutputFile}`);
console.log("");
console.log(
  "Next: import the devices file via the app to restore every non-local device\n" +
    "(SSH hosts + peer machines) under its device:<alias> source (it will NOT\n" +
    "touch local). The merged file additionally contains every machine's local:*\n" +
    "data — keep it OUT of the cloud-sync folder so the app does not ingest it\n" +
    "as a phantom 'merged' device.",
);
