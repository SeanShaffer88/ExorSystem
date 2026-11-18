import { invoke as nativeInvoke } from "@tauri-apps/api/core";
import { listen as nativeListen } from "@tauri-apps/api/event";

/** The optional browser fixture is installed only by the Vite development entry point. */
interface FixtureBridge {
  invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
  listen<T>(event: string, callback: (event: { payload: T }) => void): () => void;
}
let fixture: FixtureBridge | null = null;

export function installFixture(bridge: FixtureBridge) {
  fixture = bridge;
}

export const isDemo = () => fixture !== null;

export async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return fixture ? await fixture.invoke(command, args) as T : nativeInvoke<T>(command, args);
}

export async function listen<T>(event: string, callback: (event: { payload: T }) => void) {
  return fixture ? fixture.listen(event, callback) : nativeListen<T>(event, callback);
}
