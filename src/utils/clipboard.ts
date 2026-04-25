export async function copyToClipboard(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const el = document.createElement("textarea");
    el.value = text;
    document.body.appendChild(el);
    try {
      el.select();
      if (!document.execCommand("copy")) {
        throw new Error("Failed to copy to clipboard");
      }
    } catch {
      throw new Error("Failed to copy to clipboard");
    } finally {
      document.body.removeChild(el);
    }
  }
}
