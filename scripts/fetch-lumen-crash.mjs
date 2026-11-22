#!/usr/bin/env node
// Stages the Lumen Crash SDK Maven artifacts into the Android project.
//
// The SDK is consumed as plain Maven artifacts from a GitHub release instead of
// a package registry: no token, no registry dependency, and the pin in
// `gradle.properties` is the only thing that decides what the app builds
// against. Downloaded files land in the standard Maven layout below
// `src-tauri/gen/android/lumen-crash-maven`, which the root Gradle build adds as
// a repository.
//
// Usage:
//   node scripts/fetch-lumen-crash.mjs             # stage the pinned version
//   node scripts/fetch-lumen-crash.mjs --check     # verify only, no download
//   node scripts/fetch-lumen-crash.mjs --version 0.1.0-abc12345 --force

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, rename, rm, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPOSITORY = "Chloemlla/Project-Lumen";
const GROUP = "com.chloemlla.lumen";
const ARTIFACTS = ["lumen-crash", "lumen-crash-core"];
// `.aar` is what Gradle resolves; `.pom` and `.module` carry the metadata that
// makes transitive dependencies (Compose, activity-compose) resolvable at all.
const REQUIRED_EXTENSIONS = [".aar", ".pom", ".module"];
// Sources keep IDE navigation into the SDK working once the AAR is consumed.
const OPTIONAL_EXTENSIONS = ["-sources.jar"];

const ROOT = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const ANDROID_DIR = path.join(ROOT, "src-tauri", "gen", "android");
const GRADLE_PROPERTIES = path.join(ANDROID_DIR, "gradle.properties");
const STAGE_DIR = path.join(ANDROID_DIR, "lumen-crash-maven");

function usage(message) {
  if (message) console.error(`error: ${message}`);
  console.error(
    "usage: node scripts/fetch-lumen-crash.mjs [--check] [--force] [--version <version>]",
  );
  process.exit(2);
}

function parseArgs(argv) {
  const options = { check: false, force: false, version: null };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--check") options.check = true;
    else if (arg === "--force") options.force = true;
    else if (arg === "--version") options.version = argv[++index] ?? usage("--version needs a value");
    else if (arg === "--help" || arg === "-h") usage();
    else usage(`unknown argument: ${arg}`);
  }
  return options;
}

/** Reads the pin from gradle.properties so the app and this script cannot drift. */
async function pinnedVersion() {
  const text = await readFile(GRADLE_PROPERTIES, "utf8");
  const match = /^lumenCrashSdkVersion=(.+)$/m.exec(text);
  if (!match) usage(`lumenCrashSdkVersion is missing from ${GRADLE_PROPERTIES}`);
  return match[1].trim();
}

function releaseBaseUrl(version) {
  return `https://github.com/${REPOSITORY}/releases/download/lumen-crash-v${version}`;
}

function sha256(buffer) {
  return createHash("sha256").update(buffer).digest("hex");
}

/**
 * Downloads one release asset.
 *
 * `curl` is used rather than `fetch` because it honours the standard proxy
 * environment variables, which `fetch` ignores — and this project is developed
 * behind a proxy.
 */
function download(url, destination) {
  const result = spawnSync(
    "curl",
    ["--fail", "--silent", "--show-error", "--location", "--retry", "3", "--max-time", "300",
      "--output", destination, url],
    { stdio: ["ignore", "inherit", "inherit"] },
  );
  if (result.error) throw new Error(`failed to run curl: ${result.error.message}`);
  if (result.status !== 0) throw new Error(`download failed (${result.status}): ${url}`);
}

/** `sha256sum *` output: `<hex>  <file>` per line. */
async function readChecksums(version) {
  const cachePath = path.join(STAGE_DIR, "checksums.txt");
  await download(`${releaseBaseUrl(version)}/checksums.txt`, cachePath);
  const text = await readFile(cachePath, "utf8");
  const checksums = new Map();
  for (const line of text.split(/\r?\n/)) {
    const match = /^([0-9a-f]{64})\s+\*?(.+)$/i.exec(line.trim());
    if (match) checksums.set(match[2].trim(), match[1].toLowerCase());
  }
  return checksums;
}

function targetPath(version, artifact, extension) {
  return path.join(STAGE_DIR, GROUP.replaceAll(".", path.sep), artifact, version,
    `${artifact}-${version}${extension}`);
}

async function fileMatchesChecksum(file, expected) {
  try {
    const info = await stat(file);
    if (!info.isFile() || info.size === 0) return false;
    return sha256(await readFile(file)) === expected;
  } catch {
    return false;
  }
}

async function stageArtifact(version, artifact, extension, checksums, options) {
  const name = `${artifact}-${version}${extension}`;
  const destination = targetPath(version, artifact, extension);
  const expected = checksums.get(name);
  const required = REQUIRED_EXTENSIONS.includes(extension);

  if (!options.force && expected && (await fileMatchesChecksum(destination, expected))) {
    return { name, status: "cached" };
  }
  if (options.check) {
    if (required) throw new Error(`${name} is missing or does not match its checksum`);
    return { name, status: "absent" };
  }
  if (!expected) {
    if (required) throw new Error(`${name} is not listed in the release checksums`);
    return { name, status: "absent" };
  }

  await mkdir(path.dirname(destination), { recursive: true });
  const partial = `${destination}.part`;
  await rm(partial, { force: true });
  download(`${releaseBaseUrl(version)}/${name}`, partial);
  const actual = sha256(await readFile(partial));
  if (actual !== expected) {
    await rm(partial, { force: true });
    throw new Error(`${name}: checksum mismatch (expected ${expected}, got ${actual})`);
  }
  await rm(destination, { force: true });
  await rename(partial, destination);
  return { name, status: "downloaded" };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const version =
    options.version ?? process.env.LUMEN_CRASH_SDK_VERSION ?? (await pinnedVersion());
  console.log(`Lumen Crash SDK ${version} (${REPOSITORY})`);

  await mkdir(STAGE_DIR, { recursive: true });
  const checksums = await readChecksums(version);
  if (checksums.size === 0) throw new Error("the release checksum file is empty");

  const results = [];
  for (const artifact of ARTIFACTS) {
    for (const extension of [...REQUIRED_EXTENSIONS, ...OPTIONAL_EXTENSIONS]) {
      results.push(await stageArtifact(version, artifact, extension, checksums, options));
    }
  }

  for (const { name, status } of results) {
    console.log(`  ${status.padEnd(10)} ${name}`);
  }
  const downloaded = results.filter((entry) => entry.status === "downloaded").length;
  console.log(
    options.check
      ? `verified ${results.length} artifacts in ${STAGE_DIR}`
      : `staged ${results.length} artifacts (${downloaded} downloaded) in ${STAGE_DIR}`,
  );
}

main().catch((error) => {
  console.error(`error: ${error.message}`);
  process.exit(1);
});
