// Storybook mocks for Tauri plugin APIs.
// Each export mirrors the shape consumed by the app components.

// --- @tauri-apps/plugin-dialog ---
let openMock: () => Promise<string | string[] | null> = () =>
  Promise.resolve(null);

export async function open(): Promise<string | string[] | null> {
  return openMock();
}

export function setOpenMock(fn: () => Promise<string | string[] | null>): void {
  openMock = fn;
}

let saveMock: () => Promise<string | null> = () => Promise.resolve(null);

export async function save(): Promise<string | null> {
  return saveMock();
}

export function setSaveMock(fn: () => Promise<string | null>): void {
  saveMock = fn;
}

// --- @tauri-apps/api/app ---
let appIdentifier = 'plinth';

export async function getIdentifier(): Promise<string> {
  return appIdentifier;
}

export function setIdentifierMock(identifier: string): void {
  appIdentifier = identifier;
}

export async function getVersion(): Promise<string> {
  return '0.1.0';
}

// --- @tauri-apps/plugin-updater ---
export interface Update {
  version: string;
  date?: string;
  body?: string;
  downloadAndInstall: (onEvent?: (event: unknown) => void) => Promise<void>;
}

let checkMock: () => Promise<Update | null> = () => Promise.resolve(null);

export async function check(): Promise<Update | null> {
  return checkMock();
}

export function setCheckMock(fn: () => Promise<Update | null>): void {
  checkMock = fn;
}

// --- @tauri-apps/api/event ---
export type UnlistenFn = () => void;

export async function listen<T = unknown>(
  _event: string,
  _handler: (event: { payload: T }) => void,
): Promise<UnlistenFn> {
  return () => {};
}

// --- Reset all plugin mocks (called from preview.tsx beforeEach) ---
export function resetPluginMocks(): void {
  openMock = () => Promise.resolve(null);
  saveMock = () => Promise.resolve(null);
  appIdentifier = 'plinth';
  checkMock = () => Promise.resolve(null);
}
