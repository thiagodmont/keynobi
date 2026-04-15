import { type JSX } from "solid-js";
import { Dropdown, type MenuItem } from "@/components/ui";

export interface AvdContextMenuProps {
  trigger: JSX.Element;
  onWipe: () => void;
  onDelete: () => void;
}

export function AvdContextMenu(props: AvdContextMenuProps): JSX.Element {
  const items: MenuItem[] = [
    { label: "Wipe Data…", onClick: () => props.onWipe() },
    { separator: true },
    { label: "Delete…", onClick: () => props.onDelete(), destructive: true },
  ];
  return <Dropdown placement="top" trigger={props.trigger} items={items} />;
}
