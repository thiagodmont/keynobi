import { showDialog } from "@/components/ui/Dialog";

export type SaveDialogResult = "save" | "discard" | "cancel";
export type CloseDialogResult = "save-all" | "discard-all" | "cancel";

export function showSaveDialog(filename: string): Promise<SaveDialogResult> {
  return showDialog({
    title: "Unsaved Changes",
    message: `Do you want to save the changes you made to "${filename}"?`,
    buttons: [
      { label: "Save", value: "save", style: "primary" },
      { label: "Don't Save", value: "discard", style: "danger" },
      { label: "Cancel", value: "cancel", style: "secondary" },
    ],
  }) as Promise<SaveDialogResult>;
}

export function showCloseDialog(dirtyCount: number): Promise<CloseDialogResult> {
  const noun = dirtyCount === 1 ? "file has" : `${dirtyCount} files have`;
  return showDialog({
    title: "Save Changes Before Closing?",
    message: `${noun.charAt(0).toUpperCase()}${noun.slice(1)} unsaved changes. What would you like to do?`,
    buttons: [
      { label: "Save All", value: "save-all", style: "primary" },
      { label: "Discard All", value: "discard-all", style: "danger" },
      { label: "Cancel", value: "cancel", style: "secondary" },
    ],
  }) as Promise<CloseDialogResult>;
}
