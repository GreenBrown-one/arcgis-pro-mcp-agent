import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AddInInstallerOpenResult,
  ArcGisInstallSnapshot,
  DesktopEvent,
  DesktopSnapshot,
} from "./domain";

export type DesktopEventHandler = (event: DesktopEvent) => void;
export type UnsubscribeDesktopEvents = () => void;

export type ChatGptLoginStart = { loginId: string; authUrl: string };
export type ConversationStart = { threadId: string };
export type TurnStart = { turnId: string };

export async function getSnapshot(): Promise<DesktopSnapshot> {
  return invoke<DesktopSnapshot>("desktop_snapshot");
}

export async function rediscoverCodex(): Promise<DesktopSnapshot> {
  return invoke<DesktopSnapshot>("rediscover_codex");
}

export async function discoverArcGis(): Promise<ArcGisInstallSnapshot> {
  return invoke<ArcGisInstallSnapshot>("discover_arcgis");
}

export async function chooseArcGisExecutable(
  executable: string,
): Promise<ArcGisInstallSnapshot> {
  return invoke<ArcGisInstallSnapshot>("choose_arcgis_executable", { executable });
}

export async function selectArcGisExecutable(): Promise<ArcGisInstallSnapshot | null> {
  const executable = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "ArcGIS Pro", extensions: ["exe"] }],
  });
  if (typeof executable !== "string") return null;
  return chooseArcGisExecutable(executable);
}

export async function openAddinInstaller(): Promise<AddInInstallerOpenResult> {
  return invoke<AddInInstallerOpenResult>("open_addin_installer");
}

export async function launchArcGis(): Promise<number> {
  return invoke<number>("launch_arcgis");
}

export async function getAddinUninstallGuidance(): Promise<string> {
  return invoke<string>("addin_uninstall_guidance");
}

export async function startChatGptLogin(): Promise<ChatGptLoginStart> {
  return invoke<ChatGptLoginStart>("chatgpt_login_start");
}

export async function cancelChatGptLogin(): Promise<void> {
  await invoke("chatgpt_login_cancel");
}

export async function openExternalUrl(url: string): Promise<void> {
  await openUrl(url);
}

export async function logoutChatGpt(): Promise<void> {
  await invoke("chatgpt_logout");
}

export async function startConversation(): Promise<ConversationStart> {
  return invoke<ConversationStart>("conversation_start");
}

export async function startTurn(message: string): Promise<TurnStart> {
  return invoke<TurnStart>("turn_start", { message });
}

export async function interruptTurn(): Promise<void> {
  await invoke("turn_interrupt");
}

export async function subscribeDesktopEvents(
  handler: DesktopEventHandler,
): Promise<UnsubscribeDesktopEvents> {
  return listen<DesktopEvent>("desktop://event", (event) => handler(event.payload));
}
