// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Unit = "metric" | "imperial";

export interface Device {
  serialNumber: string;
  home: string | null;
  name: string;
  type: string;
  sensors: string[];
}

export interface SensorValue {
  sensorType: string;
  value: number;
  unit: string;
}

export interface DeviceSensors {
  serialNumber: string;
  sensors: SensorValue[];
  recorded: string | null;
  batteryPercentage: number | null;
}

export interface SensorInfo {
  sensorType: string;
  unit: string;
}

export interface RateLimit {
  limit: number | null;
  remaining: number | null;
  reset: string | null;
}

export interface PollerStatus {
  signedIn: boolean;
  paused: boolean;
  polling: boolean;
  lastPoll: number | null;
  nextPoll: number | null;
  lastError: string | null;
  rateLimit: RateLimit | null;
}

export interface AuthInfo {
  signedIn: boolean;
  method: "oauth" | "clientCredentials" | null;
  oauthAvailable: boolean;
  redirectUri: string;
}

export interface Settings {
  unit: Unit;
  pollIntervalMinutes: number;
  closeToTray: boolean;
  theme: "system" | "light" | "dark";
  oauthClientId: string | null;
  oauthClientSecret: string | null;
  redirectPort: number;
  scope: string;
}

export interface RemoteControlState {
  mode: "OFF" | "AUTO" | "SLEEP" | "BOOST" | "MANUAL";
  fanSpeed?: number;
}

export const api = {
  authInfo: () => invoke<AuthInfo>("auth_info"),
  signInOAuth: () => invoke<void>("sign_in_oauth"),
  signInClientCredentials: (clientId: string, clientSecret: string) =>
    invoke<void>("sign_in_client_credentials", { clientId, clientSecret }),
  signOut: () => invoke<void>("sign_out"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  devices: () => invoke<Device[]>("devices"),
  latestReadings: () => invoke<DeviceSensors[]>("latest_readings"),
  sensorTypes: (serial: string) => invoke<SensorInfo[]>("sensor_types", { serial }),
  series: (serial: string, sensorType: string, from: number, to: number, maxPoints: number) =>
    invoke<[number, number][]>("series", { serial, sensorType, from, to, maxPoints }),
  exportCsv: (serial: string, fileName: string, from: number, to: number) =>
    invoke<string | null>("export_csv", { serial, fileName, from, to }),
  pollerStatus: () => invoke<PollerStatus>("poller_status"),
  refreshNow: () => invoke<void>("refresh_now"),
  setPaused: (paused: boolean) => invoke<void>("set_paused", { paused }),
  remoteControl: (serial: string) => invoke<RemoteControlState | null>("remote_control", { serial }),
  setRemoteControl: (serial: string, mode: RemoteControlState) =>
    invoke<void>("set_remote_control", { serial, mode }),
  clearLocalData: () => invoke<void>("clear_local_data"),
  dataDir: () => invoke<string>("data_dir"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  openDataDir: () => invoke<void>("open_data_dir"),
};

export const events = {
  pollerStatus: (cb: (s: PollerStatus) => void): Promise<UnlistenFn> =>
    listen<PollerStatus>("poller-status", (e) => cb(e.payload)),
  readingsUpdated: (cb: () => void): Promise<UnlistenFn> => listen("readings-updated", cb),
  devicesUpdated: (cb: () => void): Promise<UnlistenFn> => listen("devices-updated", cb),
};

/** Tauri rejects command promises with the serialized Rust error, which is a string. */
export function errorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return String(err);
}
