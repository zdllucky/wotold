#!/usr/bin/env node
// Проставляет версию во все четыре файла, которые сверяет check-versions.mjs.
//
// Отдельный скрипт, а не sed в workflow: Cargo.toml и три package.json
// правятся по-разному, и «поправить регуляркой прямо в yaml» — ровно тот
// случай, когда ошибка обнаруживается уже опубликованным релизом.
//
// Использование:
//   node scripts/bump-version.mjs patch|minor|major
//   node scripts/bump-version.mjs 1.0.0
//
// Печатает новую версию в stdout — workflow забирает её оттуда.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const arg = process.argv[2];

if (!arg) {
  console.error('usage: node scripts/bump-version.mjs <patch|minor|major|X.Y.Z>');
  process.exit(2);
}

const CARGO = 'apps/desktop/src-tauri/Cargo.toml';
const TAURI_CONF = 'apps/desktop/src-tauri/tauri.conf.json';
const PKG_FILES = ['apps/desktop/package.json', 'package.json'];

const SEMVER = /^(\d+)\.(\d+)\.(\d+)$/;

function readCargoVersion() {
  const text = readFileSync(join(root, CARGO), 'utf8');
  const match = text.match(/^version\s*=\s*"([^"]+)"/m);
  if (!match) {
    console.error(`Failed to parse version from ${CARGO}`);
    process.exit(1);
  }
  return match[1];
}

function nextVersion(current, spec) {
  if (SEMVER.test(spec)) return spec;

  const parsed = current.match(SEMVER);
  if (!parsed) {
    console.error(`Current version ${current} is not X.Y.Z — pass an explicit version instead.`);
    process.exit(1);
  }
  const [major, minor, patch] = parsed.slice(1).map(Number);

  switch (spec) {
    case 'major':
      return `${major + 1}.0.0`;
    case 'minor':
      return `${major}.${minor + 1}.0`;
    case 'patch':
      return `${major}.${minor}.${patch + 1}`;
    default:
      console.error(`Unknown bump "${spec}" — expected patch, minor, major or X.Y.Z.`);
      process.exit(1);
  }
}

const current = readCargoVersion();
const next = nextVersion(current, arg);

if (next === current) {
  console.error(`Version is already ${next} — nothing to release.`);
  process.exit(1);
}

// Cargo.toml: только первый `version = "..."` в начале строки. Тот же якорь,
// что и у check-versions.mjs — версии зависимостей ниже не трогаются.
{
  const path = join(root, CARGO);
  const text = readFileSync(path, 'utf8');
  writeFileSync(path, text.replace(/^version\s*=\s*"[^"]+"/m, `version = "${next}"`));
}

// tauri.conf.json и package.json — через JSON, с сохранением отступа в 2
// пробела и завершающего перевода строки, как их держит prettier.
for (const rel of [TAURI_CONF, ...PKG_FILES]) {
  const path = join(root, rel);
  const json = JSON.parse(readFileSync(path, 'utf8'));
  json.version = next;
  writeFileSync(path, `${JSON.stringify(json, null, 2)}\n`);
}

// Cargo.lock хранит версию самого пакета и после бампа расходится с
// Cargo.toml. Ломается от этого ничего — сборка идёт без `--locked`, — но
// каждый релиз оставлял грязное дерево, и правка приезжала хвостом в
// следующий, посторонний коммит. Правим ту единственную запись, что
// относится к нашему пакету: запускать cargo ради этого не нужно, а в
// релизной джобе его и нет.
{
  const path = join(root, 'Cargo.lock');
  const text = readFileSync(path, 'utf8');
  const patched = text.replace(
    /(\[\[package\]\]\nname = "wotold-desktop"\nversion = )"[^"]+"/,
    `$1"${next}"`,
  );
  if (patched === text) {
    console.error('Cargo.lock: не нашёл запись wotold-desktop — проверь формат лок-файла.');
    process.exit(1);
  }
  writeFileSync(path, patched);
}

console.log(next);
