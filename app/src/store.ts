// The latest UiState from Rust: fetched once, then replaced on every `state-changed` event.
import { useSyncExternalStore } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "./ipc";
import { onMockState } from "./mock";
import type { UiState } from "./types";

let state: UiState | null = null;
const subscribers = new Set<() => void>();

function set(next: UiState) {
  state = next;
  subscribers.forEach((notify) => notify());
}

let started = false;
function start() {
  if (started) return;
  started = true;
  if (isTauri()) listen<UiState>("state-changed", (e) => set(e.payload));
  else onMockState(set);
  ipc.getState().then(set);
}

function subscribe(notify: () => void) {
  start();
  subscribers.add(notify);
  return () => subscribers.delete(notify);
}

/** The current UiState, or null until the first one arrives. */
export function useUiState(): UiState | null {
  return useSyncExternalStore(subscribe, () => state);
}
