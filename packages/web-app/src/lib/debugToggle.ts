// File: packages/web-app/src/lib/debugToggle.ts
//
// The ONE persisted bit of the local debug panel (story 3 R-7 / R-35): whether
// the panel is open. A per-viewer convenience in `localStorage`, nothing more.
//
//   * Only the BOOLEAN is stored — `'1'` or `'0'` under a fixed, versioned key.
//     No statistic, no device id, no state object ever reaches storage.
//   * Reads FAIL CLOSED: anything other than exactly `'1'`, or a throwing
//     storage (disabled, quota, privacy mode), reads as OFF.
//   * Writes are best-effort: a throwing storage leaves the runtime toggle
//     working and simply does not persist.
//   * The self-check is NEVER persisted: reopening the panel never re-opens a
//     camera or microphone without a fresh user gesture.

/** The storage key. Versioned so a future meaning change cannot misread an old value. */
export const DEBUG_PANEL_STORAGE_KEY = 'darktower.debugPanel.v1';

/** The storage surface used (a `Storage` subset), injectable for tests. */
export type ToggleStorage = Pick<Storage, 'getItem' | 'setItem'>;

function defaultStorage(): ToggleStorage | undefined {
  try {
    return typeof localStorage === 'undefined' ? undefined : localStorage;
  } catch {
    // Accessing `localStorage` itself throws when storage is blocked.
    return undefined;
  }
}

/** Whether the panel was left open. Fails closed. */
export function readDebugPanelOpen(storage: ToggleStorage | undefined = defaultStorage()): boolean {
  try {
    return storage?.getItem(DEBUG_PANEL_STORAGE_KEY) === '1';
  } catch {
    return false;
  }
}

/** Remember the panel's state. Best-effort; never throws. */
export function writeDebugPanelOpen(
  open: boolean,
  storage: ToggleStorage | undefined = defaultStorage(),
): void {
  try {
    storage?.setItem(DEBUG_PANEL_STORAGE_KEY, open ? '1' : '0');
  } catch {
    // Not persisted; the runtime toggle is unaffected.
  }
}
