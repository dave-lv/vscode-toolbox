// Display helpers, matching the Swift app (Components.swift, Catalog.swift).

/** Toolbox-like tile colours, picked by a stable hash of the name (same colours as the Swift app). */
const TILES = [
  "rgb(222 122 94)", // salmon
  "rgb(77 158 112)", // green
  "rgb(156 92 209)", // purple
  "rgb(79 107 237)", // blue
  "rgb(199 143 31)", // amber
  "rgb(194 69 143)", // pink
  "rgb(46 153 158)", // teal
  "rgb(140 120 92)", // brown
];

/** FNV-1a (64-bit) of the lowercased name, so a name gets the same colour in both apps. */
export function tileColor(seed: string): string {
  let h = 0xcbf29ce484222325n;
  for (const byte of new TextEncoder().encode(seed.toLowerCase())) {
    h ^= BigInt(byte);
    h = (h * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return TILES[Number(h % BigInt(TILES.length))];
}

/** One initial, or first + last word initials (`daily-assistant` → DA). */
export function initials(name: string): string {
  const words = name.split(/[^\p{L}\p{N}]+/u).filter((w) => /^\p{L}/u.test(w));
  if (words.length === 0) return name.slice(0, 1).toUpperCase();
  const first = words[0][0];
  return (words.length > 1 ? first + words[words.length - 1][0] : first).toUpperCase();
}

/** Short code on a profile tile: PHP, DE (Default), AG (Agents), … */
export function profileCode(name: string): string {
  if ([...name].length <= 3) return name.toUpperCase();
  const words = name.split(/[^\p{L}\p{N}]+/u).filter(Boolean);
  if (words.length > 1) return words.slice(0, 2).map((w) => w[0]).join("").toUpperCase();
  return name.slice(0, 2).toUpperCase();
}

const time = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
const day = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });
const dayYear = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", year: "numeric" });

/** 13:10 today, "Yesterday", Oct 16 this year, Feb 25, 2024 before. */
export function relativeDate(ms: number, now = new Date()): string {
  const date = new Date(ms);
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  if (ms >= startOfToday) return time.format(date);
  if (ms >= startOfToday - 86_400_000) return "Yesterday";
  if (date.getFullYear() === now.getFullYear()) return day.format(date);
  return dayYear.format(date);
}

/**
 * Search ranking, lower is better: name prefix < name substring < name subsequence < path substring
 * < path subsequence; null = no match. Same as toolbox-core's `fuzzy_score`.
 */
export function fuzzyScore(query: string, name: string, detail: string): number | null {
  const q = query.trim().toLowerCase();
  if (!q) return 0;
  const n = name.toLowerCase();
  const d = detail.toLowerCase();
  if (n.startsWith(q)) return 0;
  if (n.includes(q)) return 1;
  if (isSubsequence(q, n)) return 2;
  if (d.includes(q)) return 3;
  if (isSubsequence(q, d)) return 4;
  return null;
}

function isSubsequence(needle: string, haystack: string): boolean {
  let i = 0;
  for (const c of needle) {
    if (c === " ") continue;
    i = haystack.indexOf(c, i);
    if (i < 0) return false;
    i += 1;
  }
  return true;
}

export const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
