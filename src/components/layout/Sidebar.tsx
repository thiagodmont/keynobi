import { type JSX, Show } from "solid-js";
import {
  uiState,
  setActiveSidebarTab,
  type SidebarTab,
} from "@/stores/ui.store";
import Icon from "@/components/common/Icon";

interface SidebarIconProps {
  tab: SidebarTab;
  iconName: string;
  tooltip: string;
}

function SidebarIcon(props: SidebarIconProps): JSX.Element {
  const isActive = () => uiState.activeSidebarTab === props.tab;

  return (
    <button
      title={props.tooltip}
      onClick={() => setActiveSidebarTab(props.tab)}
      style={{
        width: "var(--sidebar-icon-width)",
        height: "48px",
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        color: isActive() ? "var(--text-primary)" : "var(--text-muted)",
        "border-left": isActive()
          ? "2px solid var(--accent)"
          : "2px solid transparent",
        cursor: "pointer",
        transition: "color 0.1s",
        background: "none",
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLElement).style.color = "var(--text-primary)";
      }}
      onMouseLeave={(e) => {
        if (!isActive())
          (e.currentTarget as HTMLElement).style.color = "var(--text-muted)";
      }}
    >
      <Icon name={props.iconName} size={22} />
    </button>
  );
}

interface SidebarProps {
  children?: JSX.Element;
}

export function Sidebar(props: SidebarProps): JSX.Element {
  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "row",
        height: "100%",
        "flex-shrink": "0",
      }}
    >
      {/* Icon bar */}
      <div
        style={{
          width: "var(--sidebar-icon-width)",
          background: "var(--bg-secondary)",
          display: "flex",
          "flex-direction": "column",
          "align-items": "center",
          "padding-top": "4px",
          "border-right": "1px solid var(--border)",
          "flex-shrink": "0",
        }}
      >
        <SidebarIcon tab="files" iconName="folder" tooltip="Explorer" />
        <SidebarIcon tab="search" iconName="search" tooltip="Search" />
        <SidebarIcon tab="symbols" iconName="list" tooltip="Outline" />
        <SidebarIcon tab="git" iconName="git-branch" tooltip="Source Control" />
      </div>

      {/* Panel content */}
      <Show when={uiState.sidebarVisible}>
        <div
          style={{
            width: `${uiState.sidebarWidth}px`,
            background: "var(--bg-secondary)",
            "border-right": "1px solid var(--border)",
            overflow: "hidden",
            display: "flex",
            "flex-direction": "column",
          }}
        >
          {props.children}
        </div>
      </Show>
    </div>
  );
}

export default Sidebar;
