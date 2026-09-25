import type { Device } from "@/bindings";

/**
 * Serial to auto-start logcat on when the device list changes: the selected
 * device if it is online, otherwise the first online device.
 */
export function pickAutoStartSerial(
  devices: readonly Device[],
  selectedSerial: string | null
): string | null {
  const online = devices.filter((d) => d.connectionState === "online");
  return (online.find((d) => d.serial === selectedSerial) ?? online[0])?.serial ?? null;
}
